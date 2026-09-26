//! The F´ (F Prime) reference hosts, and loading a module against them.
//!
//! `spacewasm_std`, the reference embedder in the `spacewasm` repository,
//! registers six host functions: five under `fprime_core`, standing in for an
//! F´ flight software component, and `clock_ms` under `env`. [`REFERENCE_HOSTS`]
//! is those six in one table. A row names the host, its parameters, its result,
//! the Inference declaration that binds it, and its body, and everything else
//! is built from the rows: the host set [`load`] registers, the verdict
//! [`check_imports`] gives, and every text that lists them. The six cannot be
//! described one way and run another.
//!
//! # What the hosts do
//!
//! Each host does what the reference embedder's does, in the style of its log
//! lines rather than in its exact bytes:
//!
//! - `panic(addr, len, line)` reads `len` bytes of UTF-8 at `addr`, logs
//!   `PANIC {text}:{line}` and always stops the program with a trap.
//! - `rsleep(ticks)` logs `RSLEEP {ticks}` and returns.
//! - `command(opcode, arg)` logs `COMMAND {opcode} {arg}` and answers 0.
//! - `message(ptr, len)` reads `len` bytes of UTF-8 at `ptr` and logs
//!   `MESSAGE {text}`.
//! - `telemetry(id, time_ptr, time_len, value_ptr, value_len)` writes an
//!   eleven-byte F´ time of zero at `time_ptr` — a `u16` time base, a `u8`
//!   time context, then `u32` seconds and `u32` microseconds, little-endian —
//!   logs `TELEMETRY {id}` and answers 0. It reads nothing at `value_ptr`.
//! - `clock_ms()` answers the milliseconds since the host set was built, which
//!   is when the module began loading — not when it was started — and logs
//!   nothing.
//!
//! A buffer the guest's memory does not hold, and bytes that are not UTF-8,
//! stop the program with a trap rather than failing the host, and the host
//! records why in a detail [`HostedInstance::host_trap`] reads back after a
//! call, and [`HostedStartError::host_trap`] after a start function, so a trap
//! can be reported with its reason.
//!
//! Five things differ from the reference embedder, each on purpose:
//!
//! 1. Values are logged as plain decimals, `RSLEEP 1` and `COMMAND 42 7`,
//!    where upstream prints Rust's debug form of an optional value. The
//!    `MESSAGE`, `TELEMETRY` and `PANIC` lines match upstream's byte for byte
//!    for a payload with no control characters (see 5).
//! 2. The numbers a host addresses memory with — a guest address, and the
//!    length of a buffer a host reads — are read as unsigned 32-bit numbers,
//!    as WebAssembly addresses its memory. Upstream sign-extends them, which
//!    differs for a memory of 32,768 pages or more. `telemetry` compares
//!    `time_len` as the signed `i32` its declaration gives it, so a negative
//!    one counts as short.
//! 3. The lines go to a [`HostLog`], which is standard error when it is not a
//!    recording, streamed as each call makes it.
//! 4. `telemetry` traps, before writing anything, when `time_len` is less than
//!    eleven. Upstream writes eleven bytes whatever `time_len` says, and since
//!    an array argument is passed as its address, those bytes land in the
//!    caller's frame, past the end of a shorter buffer.
//! 5. The text a `MESSAGE` or `PANIC` line carries has its control characters
//!    escaped — `\n`, `\r`, `\t` and `\0` as those two characters, every other
//!    C0 or C1 control and DEL as `\u{..}` — and printable text untouched, so a
//!    payload is always one line: an embedded newline cannot forge a `PANIC`
//!    line, and zero padding shows as `hello\0\0\0`.
//!
//! Every host is built with the interpreter's fallible constructors, answers a
//! value of the result type its row declares, and never re-enters the engine or
//! pauses it: the three conditions under which no host can panic inside the
//! interpreter.
//!
//! # Loading
//!
//! [`load`] is the one way in. It asks [`check_imports`] first, on the bytes
//! alone, and refuses a module whose imports the reference hosts do not
//! provide as declared before any of it is decoded; the interpreter's own
//! binding, which would refuse the same module while decoding it, stays behind
//! as the backstop. It then builds the host set under the session and loads
//! the module at the reference embedder's verifier bounds, with the engine
//! configuration the caller chose — and runs nothing, so no host is called by
//! a load. [`HostedModule::start`] runs the module's start function under the
//! budget the caller chose, and gives the [`HostedInstance`] that calls
//! exports, or a [`HostedStartError`] carrying what a host recorded if one
//! stopped the start function.

use std::cell::Cell;
use std::fmt::{self, Write};
use std::ops::ControlFlow;
use std::rc::Rc;
use std::time::Instant;

use inference_target_conformance::spacewasm::{
    REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH,
};
use spacewasm::{
    Engine, HostFunction, HostFunctionBreak, HostFunctionResult, HostName, HostValList,
    TrapReason, ValType, Value,
};
use wasmparser::{CompositeInnerType, FuncType, Import, Parser, Payload, TypeRef};

use crate::args::type_name;
use crate::errors::{
    HostSetError, HostedStartError, ImportProblem, InvokeError, LoadError, UnsupportedImport,
    UnsupportedImports,
};
use crate::hosts::{HostLog, HostSet, host_module, host_set};
use crate::module::{Counted, EngineConfig, Fuel, Instance, LoadedModule, Outcome, load_with};
use crate::report::{HostTrap, TrapReport, explain_refusal};
use crate::session::Session;

/// One of the F´ reference hosts.
///
/// Two rows are the same host when they carry the same two names, which is
/// how the interpreter binds an import to a host, and a row is debug-printed as
/// those two names.
pub struct ReferenceHost {
    /// The host module it is registered under.
    pub module: &'static str,
    /// Its name within that module.
    pub field: &'static str,
    /// Its parameters, in order, under the names the reference table gives
    /// them.
    pub params: &'static [Param],
    /// Its result, when it has one.
    pub result: Option<ValType>,
    /// The Inference declaration that binds it, with `N` standing for the
    /// length of a buffer the caller chooses.
    pub declaration: &'static str,
    body: Body,
}

/// One parameter of a reference host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    /// The name the reference table gives it.
    pub name: &'static str,
    /// Its WebAssembly type.
    pub ty: ValType,
}

/// What a reference host does when it is called: it is handed its own row,
/// the state the hosts of one load share, the engine and the arguments.
type Body = fn(&'static ReferenceHost, &Context, &mut Engine, &[Value]) -> HostFunctionResult;

/// The six F´ reference hosts, in the order the reference embedder registers
/// them.
pub static REFERENCE_HOSTS: [ReferenceHost; 6] = [
    ReferenceHost {
        module: "fprime_core",
        field: "panic",
        params: &[
            Param { name: "addr", ty: ValType::I32 },
            Param { name: "len", ty: ValType::I32 },
            Param { name: "line", ty: ValType::I32 },
        ],
        result: None,
        declaration: "external fn panic(text: [u8; N], len: i32, line: i32);",
        body: panic,
    },
    ReferenceHost {
        module: "fprime_core",
        field: "rsleep",
        params: &[Param { name: "ticks", ty: ValType::I64 }],
        result: None,
        declaration: "external fn rsleep(ticks: i64);",
        body: rsleep,
    },
    ReferenceHost {
        module: "fprime_core",
        field: "command",
        params: &[
            Param { name: "opcode", ty: ValType::I32 },
            Param { name: "arg", ty: ValType::I32 },
        ],
        result: Some(ValType::I32),
        declaration: "external fn command(opcode: i32, arg: i32) -> i32;",
        body: command,
    },
    ReferenceHost {
        module: "fprime_core",
        field: "message",
        params: &[
            Param { name: "ptr", ty: ValType::I32 },
            Param { name: "len", ty: ValType::I32 },
        ],
        result: None,
        declaration: "external fn message(text: [u8; N], len: i32);",
        body: message,
    },
    ReferenceHost {
        module: "fprime_core",
        field: "telemetry",
        params: &[
            Param { name: "id", ty: ValType::I32 },
            Param { name: "time_ptr", ty: ValType::I32 },
            Param { name: "time_len", ty: ValType::I32 },
            Param { name: "value_ptr", ty: ValType::I32 },
            Param { name: "value_len", ty: ValType::I32 },
        ],
        result: Some(ValType::I32),
        declaration: "external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, \
                      value: [u8; N], value_len: i32) -> i32;",
        body: telemetry,
    },
    ReferenceHost {
        module: "env",
        field: "clock_ms",
        params: &[],
        result: Some(ValType::I64),
        declaration: "external fn clock_ms() -> i64;",
        body: clock_ms,
    },
];

/// The line that explains the signatures an import refusal shows, owed
/// whenever one shows a declared signature beside a host's.
pub const LOWERING_NOTE: &str = "Signatures are WebAssembly's: an array or struct argument is \
                                 passed as its address, and every integer narrower than 64 bits \
                                 and every `bool` travels as one i32, so `text: [u8; 5]` is \
                                 `i32` here.";

/// The bytes of an F´ time: a `u16` time base, a `u8` time context, `u32`
/// seconds and `u32` microseconds.
const TIME_BYTES: i32 = 11;

impl ReferenceHost {
    /// `module.field`, as an import is written.
    #[must_use]
    pub fn name(&self) -> String {
        format!("{}.{}", self.module, self.field)
    }

    /// The parameters, named, and the result, as the reference table writes
    /// them: `(opcode: i32, arg: i32) -> i32`.
    #[must_use]
    pub fn signature(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|param| format!("{}: {}", param.name, type_name(param.ty)))
            .collect();
        let params = format!("({})", params.join(", "));
        match self.result {
            Some(result) => format!("{params} -> {}", type_name(result)),
            None => params,
        }
    }

    /// Whether a function of these parameter and result types is this host's
    /// signature.
    fn has_signature(
        &self,
        params: &[wasmparser::ValType],
        results: &[wasmparser::ValType],
    ) -> bool {
        let own_params = self.params.iter().map(|param| wasm_type(param.ty));
        let own_results = self.result.map(wasm_type);
        own_params.eq(params.iter().copied()) && own_results.into_iter().eq(results.iter().copied())
    }

    /// The host function this row registers, sharing `context` with every
    /// other host of the same load.
    fn function(&'static self, context: &Rc<Context>) -> Result<HostFunction, HostSetError> {
        let refused = |error| HostSetError::Function { name: self.name(), error };
        let name = HostName::try_from_str(self.field)
            .map_err(|error| HostSetError::Name { name: self.field.to_string(), error })?;
        let params = HostValList::try_new(&letters(self.params.iter().map(|param| param.ty)))
            .map_err(refused)?;
        let result = HostValList::try_new(&letters(self.result)).map_err(refused)?;
        let context = Rc::clone(context);
        HostFunction::try_new(name, params, result, move |engine: &mut Engine, args: &[Value]| {
            (self.body)(self, &context, engine, args)
        })
        .map_err(refused)
    }
}

/// The row as the reference table lists it:
/// `fprime_core.command(opcode: i32, arg: i32) -> i32`.
impl fmt::Display for ReferenceHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.name(), self.signature())
    }
}

impl fmt::Debug for ReferenceHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReferenceHost({})", self.name())
    }
}

impl PartialEq for ReferenceHost {
    fn eq(&self, other: &Self) -> bool {
        (self.module, self.field) == (other.module, other.field)
    }
}

impl Eq for ReferenceHost {}

/// The reference table, one indented line per host, in the order the reference
/// embedder registers them.
#[must_use]
pub fn reference_table_lines() -> Vec<String> {
    REFERENCE_HOSTS.iter().map(|host| format!("  {host}")).collect()
}

/// Checks every import of `wasm` against the reference hosts, by its two names
/// and then by its signature.
///
/// Reads the bytes alone, so it answers for a module built at any target and
/// needs no session. A module whose type or import section cannot be read is
/// left to the interpreter, which refuses it whole: this answers only whether
/// the imports it can read are the reference hosts.
///
/// # Errors
///
/// [`UnsupportedImports`] listing every import that is not a reference host at
/// its reference signature, sorted by module and then field name: a function
/// no reference host carries both names of, one that a host carries at
/// another signature, and any import that is not a function.
pub fn check_imports(wasm: &[u8]) -> Result<(), UnsupportedImports> {
    let mut types: Vec<Option<FuncType>> = Vec::new();
    let mut refused = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let Ok(payload) = payload else {
            break;
        };
        match payload {
            Payload::TypeSection(reader) => {
                for group in reader.into_iter().flatten() {
                    types.extend(group.types().map(|sub_type| match &sub_type.composite_type.inner {
                        CompositeInnerType::Func(func) => Some(func.clone()),
                        _ => None,
                    }));
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports().flatten() {
                    if let Some(problem) = problem_with(&import, &types) {
                        refused.push(UnsupportedImport {
                            module: import.module.to_string(),
                            field: import.name.to_string(),
                            problem,
                        });
                    }
                }
                break;
            }
            _ => {}
        }
    }
    match UnsupportedImports::of(refused) {
        Some(unsupported) => Err(unsupported),
        None => Ok(()),
    }
}

/// What keeps the reference hosts from providing `import`, if anything does.
///
/// An import of a type the module does not define is left to the interpreter,
/// which refuses the module for it.
fn problem_with(import: &Import<'_>, types: &[Option<FuncType>]) -> Option<ImportProblem> {
    let index = match import.ty {
        TypeRef::Func(index) | TypeRef::FuncExact(index) => index,
        TypeRef::Memory(_) => return Some(ImportProblem::NotAFunction { kind: "memory" }),
        TypeRef::Table(_) => return Some(ImportProblem::NotAFunction { kind: "table" }),
        TypeRef::Global(_) => return Some(ImportProblem::NotAFunction { kind: "global" }),
        TypeRef::Tag(_) => return Some(ImportProblem::NotAFunction { kind: "tag" }),
    };
    let Some(host) = REFERENCE_HOSTS
        .iter()
        .find(|host| host.module == import.module && host.field == import.name)
    else {
        let elsewhere = REFERENCE_HOSTS.iter().find(|host| host.field == import.name);
        return Some(ImportProblem::Unknown { elsewhere });
    };
    let declared = types.get(usize::try_from(index).ok()?)?.as_ref()?;
    if host.has_signature(declared.params(), declared.results()) {
        return None;
    }
    let names = |types: &[wasmparser::ValType]| types.iter().map(ToString::to_string).collect();
    Some(ImportProblem::Mismatch {
        params: names(declared.params()),
        results: names(declared.results()),
        host,
    })
}

impl UnsupportedImports {
    /// The lines an import refusal lists the imports in: each import's name,
    /// indented two spaces, and under it, indented four, what is wrong with it.
    ///
    /// `embedder` is how the text names the program that provides the hosts —
    /// `not a host function {embedder} provides` — written as it should
    /// appear, backticks included.
    #[must_use]
    pub fn import_lines(&self, embedder: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for import in self.imports() {
            lines.push(format!("  {}.{}", import.module, import.field));
            match &import.problem {
                ImportProblem::Unknown { elsewhere } => {
                    let mut line = format!("    not a host function {embedder} provides");
                    if let Some(host) = elsewhere {
                        let _ = write!(
                            line,
                            "; `{field}` is provided under `{module}`: use {{ {field} }} from \
                             host::{module};",
                            field = host.field,
                            module = host.module
                        );
                    }
                    lines.push(line);
                }
                ImportProblem::Mismatch { params, results, host } => {
                    lines.push(format!(
                        "    this program declares  {}",
                        declared_signature(params, results)
                    ));
                    lines.push(format!("    the host provides      {}", host.signature()));
                    let mut matching =
                        format!("    a declaration that matches: {}", host.declaration);
                    if host.declaration.contains("; N]") {
                        matching.push_str("   (N: your buffer's length)");
                    }
                    lines.push(matching);
                }
                ImportProblem::NotAFunction { kind } => lines.push(format!(
                    "    a {kind} import; the reference hosts provide functions only"
                )),
            }
        }
        lines
    }

    /// Whether any import is refused for its signature, which makes
    /// [`LOWERING_NOTE`] owed beside the lines.
    #[must_use]
    pub fn shows_signatures(&self) -> bool {
        self.imports().iter().any(|import| matches!(import.problem, ImportProblem::Mismatch { .. }))
    }

    /// Whether any function is refused for a name no reference host carries,
    /// which makes the reference table owed beside the lines.
    #[must_use]
    pub fn names_an_unknown_function(&self) -> bool {
        self.imports().iter().any(|import| matches!(import.problem, ImportProblem::Unknown { .. }))
    }
}

/// A module's side of a signature: `(i32)`, `(i32, i32) -> i32`.
fn declared_signature(params: &[String], results: &[String]) -> String {
    let params = format!("({})", params.join(", "));
    match results {
        [] => params,
        [result] => format!("{params} -> {result}"),
        results => format!("{params} -> ({})", results.join(", ")),
    }
}

/// Loads `wasm` against the F´ reference hosts, at the reference embedder's
/// verifier bounds, under `config`, logging each host call to `log`, and runs
/// nothing.
///
/// Checks the imports first, then builds the host set under the session —
/// `clock_ms` counts from that moment, the load, not from the start — then
/// decodes the module. A module the decoder refuses for want of memory is
/// measured again by the target's conformance check, which says which limit it
/// exceeds. [`HostedModule::start`] runs the start function under the budget of
/// the whole run, which [`HostedInstance::invoke`] spends the rest of.
///
/// # Errors
///
/// [`LoadError::UnsupportedImports`] before anything is decoded;
/// [`LoadError::Hosts`] when the hosts cannot be built; for a decoder refusal,
/// [`LoadError::OverLimit`] or [`LoadError::CodePagesExhausted`] when it is for
/// want of memory in a module the conformance check accepts,
/// [`LoadError::ConformanceGap`] when the check accepts a module refused for a
/// reason it should have caught, and [`LoadError::Decode`] otherwise — a
/// `memory.grow` among them, which the load tells the code builder to refuse;
/// and every other [`LoadError`] [`crate::load_with`] returns.
pub fn load<'s>(
    session: &'s mut Session,
    wasm: &[u8],
    log: HostLog,
    config: EngineConfig,
) -> Result<HostedModule<'s>, LoadError> {
    check_imports(wasm).map_err(LoadError::UnsupportedImports)?;
    let context = Rc::new(Context { log, trap: Cell::new(None), loaded_at: Instant::now() });
    let hosts = reference_host_set(session, &context).map_err(LoadError::Hosts)?;
    let module = load_with::<
        { REFERENCE_MAX_CONTROL_FRAMES as usize },
        { REFERENCE_MAX_STACK_DEPTH as usize },
    >(session, wasm, hosts, config)
    .map_err(|error| match error {
        LoadError::Decode(verdict) => explain_refusal(
            wasm,
            verdict,
            REFERENCE_MAX_CONTROL_FRAMES,
            REFERENCE_MAX_STACK_DEPTH,
            config.max_code_pages,
        ),
        other => other,
    })?;
    Ok(HostedModule { module, context, stack_words: config.stack_words })
}

/// A module loaded against the F´ reference hosts, none of it run yet.
///
/// [`HostedModule::module`] reads what it exports, so a caller can refuse a
/// call before any of the module's code has executed and before any host has
/// been called. [`HostedModule::start`] runs its start function and gives the
/// [`HostedInstance`] that calls exports; a hosted module has no call of its
/// own:
///
/// ```compile_fail,E0599
/// fn call(module: &mut inference_spacewasm_runner::fprime::HostedModule<'_>) {
///     let _ = module.invoke("main", &[]);
/// }
/// ```
pub struct HostedModule<'session> {
    module: LoadedModule<'session>,
    context: Rc<Context>,
    stack_words: usize,
}

impl<'session> HostedModule<'session> {
    /// The module itself, for what it exports and what it compiled to.
    #[must_use]
    pub fn module(&self) -> &LoadedModule<'session> {
        &self.module
    }

    /// Runs the module's start function, if it declares one, under `fuel`,
    /// the budget of the whole run, and gives the module up for the
    /// [`HostedInstance`] that calls its exports, as
    /// [`LoadedModule::start`] does.
    ///
    /// # Errors
    ///
    /// A [`HostedStartError`] when the start function does not return, holding
    /// the [`crate::StartError`], the detail a host recorded when a host
    /// stopped it, and the stack this module was loaded with, for its report.
    pub fn start(self, fuel: Fuel) -> Result<HostedInstance<'session>, HostedStartError> {
        let Self { module, context, stack_words } = self;
        match module.start(fuel) {
            Ok(instance) => Ok(HostedInstance { instance, context, stack_words }),
            Err(error) => Err(HostedStartError::new(error, context.trap.get(), stack_words)),
        }
    }
}

/// A module loaded against the F´ reference hosts whose start function has
/// returned, or that declares none: the one that calls exports.
///
/// It borrows the session its module was loaded under, and so is not `Send`:
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<inference_spacewasm_runner::fprime::HostedInstance<'static>>();
/// ```
pub struct HostedInstance<'session> {
    instance: Instance<'session>,
    context: Rc<Context>,
    stack_words: usize,
}

impl<'session> HostedInstance<'session> {
    /// The module itself, for what it exports and what it compiled to.
    #[must_use]
    pub fn module(&self) -> &LoadedModule<'session> {
        self.instance.module()
    }

    /// Calls `export` with `args` under what the start function left of the
    /// budget the module was started under, forgetting any trap detail an
    /// earlier call recorded.
    ///
    /// # Errors
    ///
    /// As [`Instance::invoke`].
    pub fn invoke(&mut self, export: &str, args: &[Value]) -> Result<Outcome, InvokeError> {
        self.context.trap.set(None);
        self.instance.invoke_within_budget(export, args)
    }

    /// Calls `export` with `args` as [`HostedInstance::invoke`] does, and
    /// counts the interpreter instructions the call takes as
    /// [`Instance::invoke_counting`] does: exactly, at the cost of one
    /// re-entry into the interpreter per instruction. A call into a host is
    /// one instruction, however much its body does.
    ///
    /// # Errors
    ///
    /// As [`Instance::invoke`].
    pub fn invoke_counting(
        &mut self,
        export: &str,
        args: &[Value],
    ) -> Result<Counted, InvokeError> {
        self.context.trap.set(None);
        self.instance.invoke_counting(export, args)
    }

    /// Why a host stopped the last call, when one did.
    #[must_use]
    pub fn host_trap(&self) -> Option<HostTrap> {
        self.context.trap.get()
    }

    /// The log the hosts write to.
    #[must_use]
    pub fn log(&self) -> &HostLog {
        &self.context.log
    }

    /// The report of a call to `export` that trapped for `reason`, with the
    /// host's detail when a host stopped it and the stack this module was
    /// loaded with.
    #[must_use]
    pub fn trap_report(&self, export: &str, reason: TrapReason) -> TrapReport {
        TrapReport::new(export, reason, self.host_trap(), self.stack_words)
    }
}

/// What the hosts of one load share with each other and with the
/// [`HostedModule`] they were loaded into, and then with the
/// [`HostedInstance`] it starts into.
struct Context {
    log: HostLog,
    trap: Cell<Option<HostTrap>>,
    loaded_at: Instant,
}

impl Context {
    /// Records why the host is stopping the program, and stops it.
    fn stop(&self, detail: HostTrap) -> HostFunctionResult {
        self.trap.set(Some(detail));
        ControlFlow::Break(HostFunctionBreak::Trap)
    }
}

/// Every reference host, grouped into one host module per module name in the
/// order the rows first name it.
fn reference_host_set(session: &Session, context: &Rc<Context>) -> Result<HostSet, HostSetError> {
    let mut modules: Vec<(&'static str, Vec<HostFunction>)> = Vec::new();
    for host in &REFERENCE_HOSTS {
        let function = host.function(context)?;
        match modules.iter_mut().find(|(module, _)| *module == host.module) {
            Some((_, functions)) => functions.push(function),
            None => modules.push((host.module, vec![function])),
        }
    }
    let modules = modules
        .into_iter()
        .map(|(module, functions)| host_module(session, module, functions, Vec::new()))
        .collect::<Result<Vec<_>, _>>()?;
    host_set(session, modules)
}

/// What a host answers when the interpreter hands it arguments its signature
/// does not declare: a trap with no detail.
///
/// The interpreter reads a host's arguments by the signature built from the
/// host's own row, so only an interpreter defect reaches this; it is an answer
/// rather than a panic because a panic inside a host unwinds through the
/// interpreter.
fn unexpected_arguments() -> HostFunctionResult {
    ControlFlow::Break(HostFunctionBreak::Trap)
}

fn panic(
    host: &'static ReferenceHost,
    context: &Context,
    engine: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[Value::I32(address), Value::I32(len), Value::I32(line)] = args else {
        return unexpected_arguments();
    };
    let detail = match guest_text(host, engine, address, len) {
        Ok(text) => {
            context.log.write_line(&format!("PANIC {text}:{line}"));
            HostTrap::Panic { host }
        }
        Err(detail) => detail,
    };
    context.stop(detail)
}

fn rsleep(
    _: &'static ReferenceHost,
    context: &Context,
    _: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[Value::I64(ticks)] = args else {
        return unexpected_arguments();
    };
    context.log.write_line(&format!("RSLEEP {ticks}"));
    ControlFlow::Continue(None)
}

fn command(
    _: &'static ReferenceHost,
    context: &Context,
    _: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[Value::I32(opcode), Value::I32(arg)] = args else {
        return unexpected_arguments();
    };
    context.log.write_line(&format!("COMMAND {opcode} {arg}"));
    ControlFlow::Continue(Some(Value::I32(0)))
}

fn message(
    host: &'static ReferenceHost,
    context: &Context,
    engine: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[Value::I32(address), Value::I32(len)] = args else {
        return unexpected_arguments();
    };
    match guest_text(host, engine, address, len) {
        Ok(text) => {
            context.log.write_line(&format!("MESSAGE {text}"));
            ControlFlow::Continue(None)
        }
        Err(detail) => context.stop(detail),
    }
}

fn telemetry(
    host: &'static ReferenceHost,
    context: &Context,
    engine: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[
        Value::I32(id),
        Value::I32(time_ptr),
        Value::I32(time_len),
        Value::I32(_),
        Value::I32(_),
    ] = args
    else {
        return unexpected_arguments();
    };
    if time_len < TIME_BYTES {
        return context.stop(HostTrap::ShortTime { host, time_len });
    }
    let address = time_ptr.cast_unsigned();
    let base = guest_offset(address);
    let memory = &engine.memory;
    // All four stores are made before any result is looked at, as the
    // reference embedder's eager `and` chain makes them, so a time running off
    // the end of memory is written up to the first field that does not fit.
    let written = memory
        .store_u16(base, 0)
        .and(memory.store_u8(base.saturating_add(2), 0))
        .and(memory.store_u32(base.saturating_add(3), 0))
        .and(memory.store_u32(base.saturating_add(7), 0));
    let Ok(()) = written else {
        let memory_bytes = memory.get_slice().len();
        return context.stop(HostTrap::TimeOutOfBounds { host, address, memory_bytes });
    };
    context.log.write_line(&format!("TELEMETRY {id}"));
    ControlFlow::Continue(Some(Value::I32(0)))
}

fn clock_ms(
    _: &'static ReferenceHost,
    context: &Context,
    _: &mut Engine,
    args: &[Value],
) -> HostFunctionResult {
    let &[] = args else {
        return unexpected_arguments();
    };
    let millis = i64::try_from(context.loaded_at.elapsed().as_millis()).unwrap_or(i64::MAX);
    ControlFlow::Continue(Some(Value::I64(millis)))
}

/// The `len` bytes at `address` in the guest's memory, as text with its
/// control characters escaped, or why `host` cannot print them.
///
/// Both numbers arrive as `i32`s and are read unsigned, as WebAssembly
/// addresses its memory.
fn guest_text(
    host: &'static ReferenceHost,
    engine: &Engine,
    address: i32,
    len: i32,
) -> Result<String, HostTrap> {
    let (address, len) = (address.cast_unsigned(), len.cast_unsigned());
    let memory = &engine.memory;
    let Ok(bytes) = memory.load(guest_offset(address), guest_offset(len)) else {
        let memory_bytes = memory.get_slice().len();
        return Err(HostTrap::OutOfBounds { host, address, len, memory_bytes });
    };
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(escape_controls(text)),
        Err(error) => Err(HostTrap::NotUtf8 { host, first_invalid: error.valid_up_to(), len }),
    }
}

/// A guest address or length as the interpreter's memory takes it. On a host
/// whose `usize` cannot hold it, the largest `usize`, which no memory reaches,
/// so the access is refused as out of bounds rather than wrapped.
fn guest_offset(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// `text` with every control character escaped, so it prints on one line:
/// `\n`, `\r`, `\t` and `\0` as those two characters, every other C0 or C1
/// control and DEL as `\u{..}`, and everything else as it is.
fn escape_controls(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            c if c.is_control() => {
                let _ = write!(escaped, "\\u{{{:x}}}", u32::from(c));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

/// `types` in the interpreter's signature alphabet: `i` for `i32`, `I` for
/// `i64`, `f` for `f32` and `d` for `f64`.
fn letters(types: impl IntoIterator<Item = ValType>) -> String {
    types
        .into_iter()
        .map(|ty| match ty {
            ValType::I32 => 'i',
            ValType::I64 => 'I',
            ValType::F32 => 'f',
            ValType::F64 => 'd',
        })
        .collect()
}

/// `ty` as the stock WebAssembly parser spells it.
fn wasm_type(ty: ValType) -> wasmparser::ValType {
    match ty {
        ValType::I32 => wasmparser::ValType::I32,
        ValType::I64 => wasmparser::ValType::I64,
        ValType::F32 => wasmparser::ValType::F32,
        ValType::F64 => wasmparser::ValType::F64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference table, in the order and with the parameter names the
    /// reference embedder's table gives.
    ///
    /// Fails if a row's names, types or result drift from the table a refusal
    /// and the help text quote.
    #[test]
    fn each_row_renders_as_the_reference_table_lists_it() {
        assert_eq!(
            reference_table_lines(),
            [
                "  fprime_core.panic(addr: i32, len: i32, line: i32)",
                "  fprime_core.rsleep(ticks: i64)",
                "  fprime_core.command(opcode: i32, arg: i32) -> i32",
                "  fprime_core.message(ptr: i32, len: i32)",
                "  fprime_core.telemetry(id: i32, time_ptr: i32, time_len: i32, value_ptr: i32, \
                 value_len: i32) -> i32",
                "  env.clock_ms() -> i64",
            ]
        );
    }

    /// Each row's declaration is the Inference text that binds it.
    #[test]
    fn each_row_carries_the_declaration_that_binds_it() {
        let declarations: Vec<&str> = REFERENCE_HOSTS.iter().map(|host| host.declaration).collect();
        assert_eq!(
            declarations,
            [
                "external fn panic(text: [u8; N], len: i32, line: i32);",
                "external fn rsleep(ticks: i64);",
                "external fn command(opcode: i32, arg: i32) -> i32;",
                "external fn message(text: [u8; N], len: i32);",
                "external fn telemetry(id: i32, mut time: [u8; 11], time_len: i32, value: [u8; N], \
                 value_len: i32) -> i32;",
                "external fn clock_ms() -> i64;",
            ]
        );
    }

    /// The signature a host is registered with is read off its row in the
    /// interpreter's alphabet.
    ///
    /// Fails if the letters stop following the types, which would register a
    /// host at a signature no row describes.
    #[test]
    fn a_rows_signature_letters_follow_its_types() {
        let signatures: Vec<(String, String)> = REFERENCE_HOSTS
            .iter()
            .map(|host| (letters(host.params.iter().map(|param| param.ty)), letters(host.result)))
            .collect();
        let expected = [("iii", ""), ("I", ""), ("ii", "i"), ("ii", ""), ("iiiii", "i"), ("", "I")];
        assert_eq!(
            signatures,
            expected.map(|(params, result)| (params.to_string(), result.to_string()))
        );
        assert_eq!(letters([ValType::F32, ValType::F64]), "fd");
    }

    /// Control characters are escaped and printable text is not.
    ///
    /// Fails if a newline, a carriage return, a tab or a NUL reaches the log
    /// raw — any of which lets a payload start a line of its own — or if the
    /// escaping reaches printable text.
    #[test]
    fn control_characters_are_escaped_and_printable_text_is_not() {
        for (raw, escaped) in [
            ("hello", "hello"),
            ("line\nPANIC forged:1", "line\\nPANIC forged:1"),
            ("a\rb\tc", "a\\rb\\tc"),
            ("hello\0\0\0", "hello\\0\\0\\0"),
            ("\u{1b}[31mred", "\\u{1b}[31mred"),
            ("\u{7f}", "\\u{7f}"),
            ("\u{85}\u{9f}", "\\u{85}\\u{9f}"),
            ("\u{1}\u{1f}", "\\u{1}\\u{1f}"),
            ("héllo, мир, 世界 ✓", "héllo, мир, 世界 ✓"),
            ("back\\slash", "back\\slash"),
            ("", ""),
        ] {
            assert_eq!(escape_controls(raw), escaped, "{raw:?}");
        }
    }

    /// Two rows are one host exactly when they carry the same two names.
    #[test]
    fn a_host_is_identified_by_its_two_names() {
        assert_eq!(REFERENCE_HOSTS[0], REFERENCE_HOSTS[0]);
        assert_ne!(REFERENCE_HOSTS[0], REFERENCE_HOSTS[3]);
        assert_eq!(REFERENCE_HOSTS[5].name(), "env.clock_ms");
    }
}
