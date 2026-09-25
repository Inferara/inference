//! Harness for driving this compiler's output against the real SpaceWasm
//! interpreter, in process, through `inference-spacewasm-runner`.
//!
//! SpaceWasm is a `no_std` flight interpreter: it decodes, validates and
//! compiles a WebAssembly 1.0 module to its own 16-bit IR, then runs that IR
//! against a fuel budget. Decoding *is* validation *is* IR compilation — there
//! is no validate-only entry point — so [`decode`] returning `Ok` is the whole
//! statement "this artifact loads on the target runtime", and a [`ParseError`]
//! carries both the offset and the reason a flight computer would have given.
//!
//! The runner is the embedder. It loads a module, runs its start function,
//! calls its exports and measures the IR it compiled to, and it returns every
//! way that can fail as a value. This file adapts it to the tier twice over.
//! The sweeps, the oracle and the host-imports rows want a harness fault to
//! fail the test where it happened, so [`decode`], [`decode_with`],
//! [`decode_with_pages`] and [`LoadedModule::invoke`] are thin wrappers that
//! keep the decoder's verdict as an `Err` and panic on everything else: an
//! interpreter that cannot be built, a start function that does not complete,
//! a call the engine refuses. The embed harness below wants a start function
//! that traps as an exit code of its own, so it loads through the runner
//! directly.
//!
//! # The single-threaded contract, and why this tier is stricter than its sibling
//!
//! The interpreter allocates through two `#[no_mangle] extern "C"` symbols
//! backed by two `static mut` globals. Upstream documents those entry points as
//! single-threaded and non-re-entrant, and puts the synchronization on the
//! embedder: reaching them from more than one thread is undefined behaviour by
//! that contract, and `cargo test` runs a binary's tests on many threads at
//! once. The runner's session is that synchronization, and this tier names it
//! [`SpaceWasmSession`].
//!
//! The contract is the reason rather than an observed write, and deliberately
//! so. The runner's allocator is zero-sized, so today neither static is written
//! after its initializer and two threads decoding at once would only read them.
//! That is what makes the session look like ceremony to a reader who checks —
//! and it stops being one the moment the allocator carries state, which the
//! bounded page allocator a flight embedder actually ships would give it.
//!
//! The session is deliberately stricter than the Soroban tier's plain
//! `session()` guard next door. There, a forgotten lock costs a flaky host;
//! here it puts the harness outside the library's stated contract, so the lock
//! is not a convention a test is asked to remember: it is a token no test can
//! forge, and the only route to a loaded module is
//! [`SpaceWasmSession::acquire`] followed by a load. A [`LoadedModule`]
//! additionally *borrows* the session it was loaded under, so a module cannot
//! outlive the lock that protects the allocator it will be freed through — the
//! case a bare token would still let a test write.
//!
//! Building a host set takes the session as well, since [`host_module`] and
//! [`host_set`] allocate through the same allocator, so a row registering
//! hosts builds them after `acquire` and before the load that consumes them.
//! One route is guarded by convention alone: the interpreter's own public
//! constructors, `HostFunction::try_new` among them, allocate too, and no
//! signature in this file stops a test calling them. Every caller in this tier
//! builds its host functions while it holds the session.
//!
//! The allocator symbols are the runner's to define, once. No binary that
//! links it defines them again — not the corpus-sweep binary this file sits
//! beside, not the `spacewasm-embed` example, not the CLI matrix that drives
//! that example's entry point in process — and the runner's own tests read
//! every Rust source in the workspace for a second definition.
//!
//! # The embedder harness
//!
//! [`run`] is the whole body of the `spacewasm-embed` example: an embedder
//! reduced to what a mission integrator does by hand — load an artifact, call
//! an export, watch it return, trap or run out of fuel, and measure the IR it
//! compiled to. It lives in this file rather than in the example so the CLI
//! matrix can drive it in process; shelling out to `cargo run --example` from
//! inside `cargo test` would contend for the build lock the test run is already
//! holding.
//!
//! ```text
//! spacewasm-embed <module.wasm> [--invoke NAME [ARG…]] [--fuel N] [--stats [--json]]
//!                 [--host MODULE.FIELD=PARAMS[:RESULT]]…
//! ```
//!
//! `--stats` reports what the artifact cost the interpreter, and `--json`
//! prints that measurement as **one line**, always the first line of stdout, so
//! `head -1` stays a complete reader even when `--invoke` prints beneath it. It
//! is printed *instead of* the export listing a bare run gives, since asking
//! for the measurement is asking for it to be the report.
//! The keys are stable, and this table is their definition:
//!
//! | key | meaning |
//! |---|---|
//! | `code_pages` | IR pages the code builder filled |
//! | `ir_words` | sixteen-bit IR words written across them |
//! | `ir_bytes` | `ir_words` × 2 |
//! | `wasm_bytes` | the artifact's size on disk |
//! | `ir_bytes_per_wasm_byte` | `ir_bytes / wasm_bytes`, unrounded |
//!
//! `code_pages` and `ir_words` come out of upstream's own two formulas — pages
//! held, and every page but the last counted full plus the writer's offset into
//! the last — so they are comparable line for line with what `spacewasm_std`'s
//! tool prints for the same artifact. The ratio is not, and is deliberately not
//! spelled the way upstream spells its own. Upstream's *compilation ratio*
//! divides the live bytes its bounded `PageAllocator` holds — the engine, the
//! guest memory and the module metadata as much as the IR — by the artifact's
//! size, and this harness cannot produce that figure, because it runs on the
//! runner's unbounded allocator, which this tier needs and which keeps no
//! statistics. What is measured here is the compiled IR alone against the
//! WebAssembly it was compiled from, so it is named for what it is: a number
//! that is not upstream's must not travel into a log under upstream's name,
//! and a JSON key travels without the sentence that would have explained it.
//!
//! Benchmark *tracking* — a history file the way the raytracer keeps one — is
//! not here; the JSON line is the hook a tracker would read, which is why it
//! carries the ratio unrounded. Two decimals is around eight percent of the
//! figure a small module produces, so a rounded key would hold still across
//! exactly the regressions a tracker exists to catch. The rendering a person
//! reads rounds, where a reader is not a series.
//!
//! # Host-import stubs
//!
//! A module importing a function loads only against a host that supplies it:
//! the interpreter binds each import by its two names and its exact signature
//! while it decodes. `--host MODULE.FIELD=PARAMS[:RESULT]` registers a stub
//! under those names, with the signature spelled in the interpreter's own
//! alphabet — one character per value type, `i` for `i32`, `I` for `i64`, `f`
//! for `f32` and `d` for `f64` — so an artifact binding
//! `use { f } from host::<module>;` can be loaded and called here instead of
//! only being named as a load failure. `--host env.clock_ms=:I` is a nullary
//! import returning an `i64`; `--host fprime_core.command=ii:i` takes two
//! `i32` and returns one.
//!
//! A stub is not an embedder. It returns zero of its result type, never
//! pauses, never traps and never reads guest memory — a pointer argument is
//! logged as a signed decimal, as a `NAME = value` result is, so an `i32`
//! address at or above 2^31 prints negative — and each call is printed to
//! stderr as `host call: module.field(arg, …)`. Which host functions a program
//! called, with which arguments and in which order, is what this harness can
//! show; what a host answers is the embedder's to decide, and a stub inventing
//! an answer would print results no flight software computes.
//!
//! Every limit a spec can exceed belongs to the interpreter, so each is asked
//! of the interpreter's own constructor and refused naming that constructor's
//! error and the constant behind it: a name longer than `HOST_MODULE_NAME_CAP`
//! or `HOST_FUNCTION_NAME_CAP` bytes, more than `MAX_HOST_FUNCTION_PARAMS`
//! parameters, more than one result. None of those numbers is written down
//! here, so none of them can drift from the interpreter this tier pins.

use std::cell::RefCell;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use inference_spacewasm_runner::{
    self as runner, EngineConfig, ExportedFunction, HostSetError, InvokeError, IrStats, LoadError,
    REFERENCE_MAX_CODE_PAGES, REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH,
    ReexportedImport, coerce_arguments, render, type_name,
};
pub use inference_spacewasm_runner::{Session as SpaceWasmSession, host_module, host_set};
use rustc_hash::{FxHashMap, FxHashSet};
use spacewasm::{
    Engine, HOST_FUNCTION_NAME_CAP, HOST_MODULE_NAME_CAP, HostFunction, HostFunctionError,
    HostModule, HostModuleRef, HostName, HostNameError, HostValList, MAX_HOST_FUNCTION_PARAMS,
    ParseError, SectionKind, TrapReason, ValType, Value,
};

/// Control frames the reference embedder's verifier admits, as the const
/// generic the loaders take.
///
/// `spacewasm_std`, upstream's own `std` embedding, is the configuration this
/// tier reports against: quoting a number nobody ships would make every depth
/// finding unactionable. The number is the conformance checker's, which the
/// runner re-exports.
pub const EMBEDDER_MAX_CONTROL_FRAMES: usize = REFERENCE_MAX_CONTROL_FRAMES as usize;

/// Operand-stack depth the reference embedder's verifier admits. See
/// [`EMBEDDER_MAX_CONTROL_FRAMES`].
pub const EMBEDDER_MAX_STACK_DEPTH: usize = REFERENCE_MAX_STACK_DEPTH as usize;

/// Words the interpreter's value stack holds.
///
/// Larger than `spacewasm_std`'s 1024 because this tier runs the whole codegen
/// corpus rather than one hand-picked program, and a stack bound reached by a
/// recursive fixture would be reported as a trap — a difference from `wasmtime`
/// that says nothing about either engine's semantics.
const STACK_WORDS: usize = 64 * 1024;

/// The engine every load in this tier builds: [`STACK_WORDS`] of value stack,
/// and the reference embedder's IR page budget.
const ENGINE: EngineConfig =
    EngineConfig { stack_words: STACK_WORDS, max_code_pages: REFERENCE_MAX_CODE_PAGES };

/// The instruction budget every invocation in this tier runs under, and every
/// start function.
///
/// Large enough that no corpus fixture reaches it — an [`Outcome::OutOfFuel`]
/// is reported as a failure naming the function rather than tolerated — and
/// finite so a lowering bug that produces an unterminated loop fails the suite
/// instead of hanging it.
pub const FUEL: usize = 100_000_000;

// ---------------------------------------------------------------------------
// The tier's loaders
// ---------------------------------------------------------------------------

/// A module the runner loaded, whose calls panic on anything but an outcome.
///
/// Named for what it is rather than `Decoded`, which the Soroban tier next door
/// already uses for a decoded `Val`.
pub struct LoadedModule<'session>(runner::LoadedModule<'session>);

/// What running an exported function did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It returned, with its result if it declared one.
    Value(Option<Value>),
    /// It trapped. The reason is carried so a failure message can name it; a
    /// comparison against another engine compares only that both trapped, since
    /// no two engines share a trap vocabulary.
    Trap(TrapReason),
    /// It was still running when the fuel budget ran out.
    OutOfFuel,
}

/// Decodes `wasm` under the reference embedder configuration with no host
/// module registered.
///
/// The `allow` is the counterpart of the one on [`run`]. Three binaries compile
/// this file, and the two that run a command line load through the runner
/// directly and never reach these loaders; seeding liveness here covers
/// [`decode_with`] and [`decode_with_pages`] behind it.
///
/// # Errors
///
/// Returns the decoder's own [`ParseError`], carrying the byte offset and the
/// validation reason.
///
/// # Panics
///
/// For the reasons [`decode_with_pages`] does.
#[allow(dead_code)]
pub fn decode<'s>(
    session: &'s mut SpaceWasmSession,
    wasm: &[u8],
) -> Result<LoadedModule<'s>, ParseError> {
    decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
        session,
        wasm,
        spacewasm::Vec::zero(),
    )
}

/// [`decode`] with the verifier's two depth bounds and the host set chosen by
/// the caller.
///
/// The bounds are const generics in the decoder because they size a stack the
/// verifier walks with, so a tighter embedder is expressed by decoding again
/// rather than by reading a number off a report. The host set is a parameter
/// for the callers that register one: the host-imports tier, which runs a
/// compiled program against recording hosts, and the oracle rows whose module
/// declares an import. Built with [`host_module`] and [`host_set`].
///
/// # Errors
///
/// Returns the decoder's own [`ParseError`].
///
/// # Panics
///
/// For the reasons [`decode_with_pages`] does.
pub fn decode_with<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
) -> Result<LoadedModule<'s>, ParseError> {
    decode_with_pages::<CONTROL_FRAMES, STACK_DEPTH>(session, wasm, hosts, REFERENCE_MAX_CODE_PAGES)
}

/// [`decode_with`] with the code builder's IR page budget chosen by the caller
/// as well.
///
/// The reference budget is `spacewasm_std`'s, and it is the right budget for
/// every question about an artifact this compiler could write. It is the wrong
/// one for a module built to sit past an index the interpreter narrows: that
/// needs more than 65,536 function bodies, which is more compiled IR than a
/// flight configuration holds, and a row decoding it under the reference budget
/// would record a page-budget refusal while saying nothing about the index.
///
/// The budget is the embedder's own choice — `CompilerOptions::max_code_pages`
/// is what a mission integrator sets — so widening it is a parameter here
/// rather than a second allocator or a second session: the allocator contract
/// and the single-threaded lock are untouched, and a caller passing a larger
/// number is describing a larger flight computer, not evading anything.
///
/// # Errors
///
/// Returns the decoder's own [`ParseError`].
///
/// # Panics
///
/// Panics on every other load failure the runner reports, since none is a
/// verdict about `wasm` this tier compares: an interpreter that cannot be
/// built at all, or a start function that traps, pauses or outlasts [`FUEL`].
/// This compiler emits no start function, so the last is a surprise in a
/// hand-written module rather than a comparable outcome.
pub fn decode_with_pages<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
    max_code_pages: usize,
) -> Result<LoadedModule<'s>, ParseError> {
    let config = EngineConfig { max_code_pages, ..ENGINE };
    match runner::load_with::<CONTROL_FRAMES, STACK_DEPTH>(session, wasm, hosts, FUEL, config) {
        Ok(module) => Ok(LoadedModule(module)),
        Err(LoadError::Decode(verdict)) => Err(verdict),
        Err(fault) => panic!("{fault}"),
    }
}

impl LoadedModule<'_> {
    /// Every function this module exports, in export-section order, leaving
    /// out a host import it exports again.
    #[must_use]
    pub fn exported_functions(&self) -> Vec<ExportedFunction> {
        self.0.exported_functions()
    }

    /// Every host import this module exports again, in export-section order.
    ///
    /// Only the command line asks this, so in the sweep binary it is kept live
    /// by the `allow` on [`run`] rather than by a caller.
    #[must_use]
    pub fn exported_host_imports(&self) -> Vec<ReexportedImport> {
        self.0.exported_host_imports()
    }

    /// Calls `export` with `args` under a `fuel`-instruction budget.
    ///
    /// # Panics
    ///
    /// Panics if the runner cannot make the call: `export` is not an exported
    /// function of this module, it resolves to a host import rather than a
    /// function with a WebAssembly body, the argument list does not match its
    /// signature, a registered host function pauses the call, or the
    /// interpreter finishes a function that declares a result without leaving
    /// one — all harness or interpreter faults, not program outcomes.
    pub fn invoke(&mut self, export: &str, args: &[Value], fuel: usize) -> Outcome {
        match self.0.invoke(export, args, fuel) {
            Ok(runner::Outcome::Returned(value)) => Outcome::Value(value),
            Ok(runner::Outcome::Trapped(reason)) => Outcome::Trap(reason),
            Ok(runner::Outcome::OutOfFuel) => Outcome::OutOfFuel,
            Err(fault) => panic!("{fault}"),
        }
    }
}

/// Measures the IR `module` compiled to, against the `wasm_len` bytes it came
/// from.
#[must_use]
pub fn ir_stats(module: &LoadedModule<'_>, wasm_len: usize) -> IrStats {
    runner::ir_stats(&module.0, wasm_len)
}

// ---------------------------------------------------------------------------
// The embedder harness
// ---------------------------------------------------------------------------

/// How to call the harness, printed under every argument-level refusal.
///
/// Spelled apart from [`exit::USAGE`], which is the code the same refusal exits
/// with: one file holding two `USAGE`s would leave a reader to tell a banner
/// from an exit code by its path.
const USAGE_LINE: &str = "usage: spacewasm-embed <module.wasm> [--invoke NAME [ARG…]] [--fuel N] \
                          [--stats [--json]] [--host MODULE.FIELD=PARAMS[:RESULT]]…";

/// `--host`'s own spelling, which every refusal of a spec's shape repeats.
const HOST_FORM: &str = "MODULE.FIELD=PARAMS[:RESULT]";

/// What the harness answers the shell with.
///
/// Every way a run the interpreter accepted and started can end badly gets a
/// code of its own. A module that never loaded, one that loaded and trapped,
/// and one that was still running when the budget ran out are three different
/// facts about a mission's artifact, and a single non-zero code would flatten
/// them into "it did not work" — which is the one thing an integrator already
/// knows.
///
/// A module's start function runs as part of its load, and a start function
/// that traps is the same fact about an artifact as an invocation that traps,
/// so it exits with [`exit::TRAP`] and its line names the start function
/// rather than an export. The rest of what can stop a load is outside the
/// guarantee rather than an eighth code inside it: an interpreter that cannot
/// be built at all, or a start function that pauses or is still running after
/// [`FUEL`](super::FUEL) instructions — which `--fuel` does not govern — is a
/// harness surprise, since this compiler declares no start function, and it
/// panics; see [`run_with`]'s own `# Panics`.
pub mod exit {
    /// The module loaded, and any invocation returned.
    pub const OK: u8 = 0;
    /// The command line could not be read.
    pub const USAGE: u8 = 2;
    /// The module file could not be read.
    pub const UNREADABLE: u8 = 3;
    /// The interpreter refused the bytes.
    pub const DECODE: u8 = 4;
    /// The module loaded but exports no function by that name that the
    /// interpreter can invoke: none at all, or a host import it re-exports.
    pub const NO_SUCH_EXPORT: u8 = 5;
    /// The invocation trapped, or the module's start function did.
    pub const TRAP: u8 = 6;
    /// The invocation was still running when the fuel ran out.
    pub const OUT_OF_FUEL: u8 = 7;

    /// Every code above, in one place a collision check can read.
    ///
    /// The distinction between these codes is the whole point of having more
    /// than one, and an equality between two names of a single value is not a
    /// distinction — so a test asks whether any two collide. Keeping the list
    /// beside the constants is what makes an eighth code added without a row
    /// here a visible omission rather than an unguarded one.
    ///
    /// The `allow` is the same one [`run`] carries and for the same reason:
    /// three binaries compile this file, and this list is read by the one that
    /// drives the command line.
    #[allow(dead_code)]
    pub const ALL: [u8; 7] = [OK, USAGE, UNREADABLE, DECODE, NO_SUCH_EXPORT, TRAP, OUT_OF_FUEL];
}

/// One parsed command line.
struct Command {
    module: PathBuf,
    invoke: Option<Invocation>,
    fuel: usize,
    stats: bool,
    json: bool,
    hosts: Vec<HostSpec>,
}

/// The export to call and the arguments to call it with, as written.
///
/// The arguments stay text until the module is loaded: what `-1` means depends
/// on the declared parameter type, which only the artifact knows.
struct Invocation {
    export: String,
    args: Vec<String>,
}

/// One `--host MODULE.FIELD=PARAMS[:RESULT]`, split into its parts but not yet
/// judged.
///
/// Only the shape is read off the command line. Whether the names fit and the
/// letters spell a signature a host function can have is the interpreter's to
/// say, through the constructors [`host_stubs`] calls, and the last of those
/// boxes the stub through the interpreter's allocator, which this harness uses
/// only while a [`SpaceWasmSession`] is held — the module documentation says
/// why that is a convention rather than a token — and none is held while a
/// command line is read. So every one of those judgments is made there, in
/// one place, rather than half of them here.
#[derive(Clone)]
struct HostSpec {
    module: String,
    field: String,
    params: String,
    /// Empty when the spec declares no result.
    result: String,
}

impl HostSpec {
    /// Splits `text` at its first `=`, the name at its last dot and the
    /// signature at its first `:`.
    ///
    /// The last dot rather than the first because a field is the one name
    /// that may not contain one: a module named `a.b` is spelled `a.b.f`. The
    /// letters are left for the interpreter to read, so a stray second `:` is
    /// refused there as a character that names no value type.
    fn parse(text: &str) -> Result<Self, String> {
        let refused = |why: &str| format!("`--host` takes {HOST_FORM}, and `{text}` {why}");
        let Some((name, signature)) = text.split_once('=') else {
            return Err(refused("has no `=`"));
        };
        let Some((module, field)) = name.rsplit_once('.') else {
            return Err(refused("has no `.` between a module and a field"));
        };
        if module.is_empty() {
            return Err(refused("names no module"));
        }
        if field.is_empty() {
            return Err(refused("names no field"));
        }
        let (params, result) = match signature.split_once(':') {
            Some((_, "")) => return Err(refused("has a `:` and no result type after it")),
            Some((params, result)) => (params, result),
            None => (signature, ""),
        };
        Ok(Self {
            module: module.to_string(),
            field: field.to_string(),
            params: params.to_string(),
            result: result.to_string(),
        })
    }

    /// Whether this spec registers its stub under `module` and `field`.
    ///
    /// The two names are compared apart and never as one joined string, which
    /// would let the module `a.b` with the field `c` answer for the module `a`
    /// with the field `b.c` — an import no spec can name.
    fn names_the_same_import(&self, module: &str, field: &str) -> bool {
        self.module == module && self.field == field
    }

    /// The spec as it would be written on the command line.
    fn text(&self) -> String {
        format!("{}.{}={}", self.module, self.field, signature_text(&self.params, &self.result))
    }
}

/// A signature in `--host`'s spelling, `PARAMS[:RESULT]`.
fn signature_text(params: &str, result: &str) -> String {
    if result.is_empty() {
        params.to_string()
    } else {
        format!("{params}:{result}")
    }
}

impl Command {
    /// Reads `argv`, which excludes the program name.
    ///
    /// A token starting with `--` ends `--invoke`'s argument list, which is what
    /// lets `--invoke f -1 --fuel 10` mean what it looks like: a negative
    /// argument is one hyphen and an option is two.
    ///
    /// Two specs naming one import are refused, keyed on the two names apart
    /// for the reason [`HostSpec::names_the_same_import`] gives. The pairs seen
    /// are hashed rather than searched, because a command line reaching the
    /// interpreter's per-module function bound carries tens of thousands of
    /// specs.
    fn parse(argv: &[String]) -> Result<Self, String> {
        let Some(first) = argv.first() else {
            return Err("no module was given".to_string());
        };
        if first.starts_with("--") {
            return Err(format!("expected a module path, found the option `{first}`"));
        }

        let mut command = Self {
            module: PathBuf::from(first),
            invoke: None,
            fuel: FUEL,
            stats: false,
            json: false,
            hosts: Vec::new(),
        };

        let value = |index: usize, option: &str, what: &str| -> Result<&str, String> {
            argv.get(index)
                .map(String::as_str)
                .filter(|token| !token.starts_with("--"))
                .ok_or_else(|| format!("`{option}` needs {what}"))
        };

        let mut imports_named: FxHashSet<(String, String)> = FxHashSet::default();
        let mut index = 1;
        while index < argv.len() {
            match argv[index].as_str() {
                "--invoke" => {
                    if command.invoke.is_some() {
                        return Err(
                            "`--invoke` was given twice; the harness calls one export per run"
                                .to_string(),
                        );
                    }
                    let export = value(index + 1, "--invoke", "the name of an exported function")?;
                    index += 2;
                    let mut args = Vec::new();
                    while let Some(arg) = argv.get(index).filter(|a| !a.starts_with("--")) {
                        args.push(arg.clone());
                        index += 1;
                    }
                    command.invoke = Some(Invocation { export: export.to_string(), args });
                }
                "--fuel" => {
                    let raw = value(index + 1, "--fuel", "an instruction budget")?;
                    command.fuel = raw.parse::<usize>().map_err(|_| {
                        format!("`--fuel` takes a decimal instruction budget, not `{raw}`")
                    })?;
                    index += 2;
                }
                "--stats" => {
                    command.stats = true;
                    index += 1;
                }
                "--json" => {
                    command.json = true;
                    index += 1;
                }
                "--host" => {
                    let spec = HostSpec::parse(value(index + 1, "--host", HOST_FORM)?)?;
                    if !imports_named.insert((spec.module.clone(), spec.field.clone())) {
                        return Err(format!(
                            "`--host {}.{}` was given twice; an import is bound to one stub",
                            spec.module, spec.field
                        ));
                    }
                    command.hosts.push(spec);
                    index += 2;
                }
                other => return Err(format!("unknown option `{other}`")),
            }
        }

        if command.json && !command.stats {
            return Err(
                "`--json` chooses the shape of `--stats`, so it needs `--stats` beside it"
                    .to_string(),
            );
        }
        Ok(command)
    }
}

/// The `spacewasm-embed` example, whole.
///
/// `argv` excludes the program name. The return value is the process's, and
/// [`exit`] is what each code means.
///
/// Acquires the interpreter session itself, so a caller already holding one
/// would deadlock; every caller in this repository is a `main` or a test that
/// holds nothing.
///
/// The `allow` is what keeps this file shared. Three binaries compile it and
/// only two call a command line: the corpus sweeps next door include it for the
/// loaders above and reach none of this, and in a binary crate an item nothing
/// reaches is dead however public it is. Seeding liveness here covers
/// everything below, which is reachable from this function and from nowhere
/// else.
#[allow(dead_code)]
#[must_use]
pub fn run(argv: Vec<String>) -> ExitCode {
    // Locked once for the whole run rather than once per line, so nothing can
    // interleave between the measurement and the result it belongs to.
    let (stdout, stderr) = (std::io::stdout(), std::io::stderr());
    ExitCode::from(run_with(&argv, &mut stdout.lock(), &mut stderr.lock()))
}

/// [`run`] with both output streams supplied, so a test can read what a run
/// printed instead of shelling out to see it.
///
/// # Panics
///
/// Panics when the interpreter cannot be built at all, when a module's start
/// function pauses or is still running after [`FUEL`] instructions, and for the
/// reasons [`LoadedModule::invoke`] does. None of them is a verdict about the
/// artifact.
#[must_use]
pub fn run_with(argv: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let command = match Command::parse(argv) {
        Ok(command) => command,
        Err(message) => {
            line(err, &format!("error: {message}"));
            line(err, USAGE_LINE);
            return exit::USAGE;
        }
    };
    execute(&command, out, err)
}

/// Writes one line, ignoring a failed write.
///
/// The exit code is this harness's answer and the stream is a courtesy: a
/// closed pipe must not turn a program that ran into a program that failed,
/// and a failure to report has nowhere left to be reported to.
fn line(sink: &mut dyn Write, text: &str) {
    let _ = writeln!(sink, "{text}");
}

/// Loads the module, then does what the command line asked of it.
///
/// The session is claimed before the module file is read, because the
/// `--host` stubs are the last command-line judgment that does not depend on
/// the artifact, and building them needs the interpreter's allocator: a spec
/// the interpreter refuses is a usage error reported before anything is
/// loaded. Only argument coercion, which needs the export's declared types, is
/// judged after the load.
///
/// The module is loaded through the runner rather than through [`decode_with`],
/// because the runner reports a start function that traps as a load failure of
/// its own, and that is an exit code here rather than a panic.
fn execute(command: &Command, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut session = SpaceWasmSession::acquire();
    let calls = HostCalls::default();
    let hosts = match host_stubs(&session, &command.hosts, &calls) {
        Ok(hosts) => hosts,
        Err(message) => {
            line(err, &format!("error: {message}"));
            line(err, USAGE_LINE);
            return exit::USAGE;
        }
    };

    let wasm = match std::fs::read(&command.module) {
        Ok(wasm) => wasm,
        Err(e) => {
            line(err, &format!("error: cannot read {}: {e}", command.module.display()));
            return exit::UNREADABLE;
        }
    };

    let loaded = runner::load_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
        &mut session,
        &wasm,
        hosts,
        FUEL,
        ENGINE,
    );
    // A start function runs inside the load, and the calls it made are printed
    // before anything else: before the line saying how it ended when it
    // trapped, and before anything the command line asked for when it
    // returned.
    let mut module = match loaded {
        Ok(module) => LoadedModule(module),
        Err(LoadError::Decode(verdict)) => {
            report_decode_failure(&session, &command.module, &wasm, &verdict, &command.hosts, err);
            return exit::DECODE;
        }
        Err(LoadError::StartTrapped(reason)) => {
            calls.flush(err);
            line(
                err,
                &format!(
                    "error: {} does not load under the SpaceWasm interpreter: its start function \
                     trapped: {reason:?}",
                    command.module.display()
                ),
            );
            return exit::TRAP;
        }
        Err(fault) => panic!("{fault}"),
    };
    calls.flush(err);

    // Before the invocation, and unconditionally: a trapping call must not take
    // the measurement of the module it trapped in down with it.
    if command.stats {
        let stats = ir_stats(&module, wasm.len());
        if command.json {
            line(out, &json_line(&stats));
        } else {
            for text in human_lines(&stats) {
                line(out, &text);
            }
        }
    }

    let functions = module.exported_functions();
    let reexported = module.exported_host_imports();
    let Some(invocation) = command.invoke.as_ref() else {
        if !command.stats {
            // A run that asked for nothing still says what it found, or a decode
            // check would be indistinguishable from a harness that did nothing.
            line(out, &format!("loaded {}", command.module.display()));
            report_exports(&functions, &reexported, out);
        }
        return exit::OK;
    };

    let Some(signature) = functions.iter().find(|f| f.name == invocation.export) else {
        let path = command.module.display();
        let message = match reexported.iter().find(|export| export.name == invocation.export) {
            Some(export) => format!(
                "error: {path}: {}",
                InvokeError::HostReexport {
                    export: invocation.export.clone(),
                    import: export.import.clone(),
                }
            ),
            None => format!("error: {path} exports no function `{}`", invocation.export),
        };
        line(err, &message);
        report_exports(&functions, &reexported, err);
        return exit::NO_SUCH_EXPORT;
    };

    let args = match coerce_arguments(signature, &invocation.args) {
        Ok(args) => args,
        Err(refusal) => {
            line(err, &format!("error: {refusal}"));
            line(err, USAGE_LINE);
            return exit::USAGE;
        }
    };

    let outcome = module.invoke(&invocation.export, &args, command.fuel);
    calls.flush(err);
    match outcome {
        Outcome::Value(None) => {
            line(out, &format!("{} = (unit)", invocation.export));
            exit::OK
        }
        Outcome::Value(Some(value)) => {
            line(out, &format!("{} = {}", invocation.export, render(value)));
            exit::OK
        }
        Outcome::Trap(reason) => {
            line(err, &format!("error: `{}` trapped: {reason:?}", invocation.export));
            exit::TRAP
        }
        Outcome::OutOfFuel => {
            line(
                err,
                &format!(
                    "error: `{}` was still running after {} instructions; raise `--fuel` if the \
                     program is meant to run longer",
                    invocation.export, command.fuel
                ),
            );
            exit::OUT_OF_FUEL
        }
    }
}

/// Says what the module exports, under a line that has just said it does not
/// export what was asked for — or that it loaded.
///
/// A host import the module exports again is listed after the functions the
/// interpreter can invoke, and named for the import behind it rather than
/// given a signature: a signature is what a reader copies back onto the command
/// line, and that one would be refused.
fn report_exports(
    functions: &[ExportedFunction],
    reexported: &[ReexportedImport],
    sink: &mut dyn Write,
) {
    if functions.is_empty() && reexported.is_empty() {
        line(sink, "  it exports no functions");
        return;
    }
    line(sink, "  it exports:");
    for function in functions {
        line(sink, &format!("    {}", describe(function)));
    }
    for export in reexported {
        line(
            sink,
            &format!(
                "    {}: the host import `{}`, which the interpreter cannot invoke",
                export.name, export.import
            ),
        );
    }
}

/// The calls the `--host` stubs received, in the order they arrived, until the
/// harness prints them.
///
/// A stub cannot print its own line. The interpreter owns a stub for as long
/// as the module stays loaded, so its closure has to be `'static`, while the
/// stream a run reports to is borrowed only for the length of [`run_with`]'s
/// call. The one stream a `'static` closure could reach is the process's own
/// stderr, and that is not where an in-process caller reads: the CLI matrix
/// hands [`run_with`] a buffer, and a line written past it would never be
/// seen. So each stub appends to this log, and the harness drains it into the
/// run's stream the moment the interpreter returns and before the line saying
/// how the call ended — a reader sees what a program asked of its host, in the
/// order it asked, and then what came of it.
#[derive(Clone, Default)]
struct HostCalls(Rc<RefCell<Vec<String>>>);

impl HostCalls {
    /// Appends one call.
    fn record(&self, call: String) {
        self.0.borrow_mut().push(call);
    }

    /// Writes every call recorded so far to `sink`, oldest first, and forgets
    /// them.
    fn flush(&self, sink: &mut dyn Write) {
        for call in self.0.borrow_mut().drain(..) {
            line(sink, &call);
        }
    }
}

/// The host set `--host` asked for: a stub per spec, and one host module per
/// module name holding every stub registered under it, in the order given.
///
/// The decode-failure report calls this too: once per suggested spec, through
/// [`stub_for`], which reaches every refusal [`stub`] and [`host_module`] make
/// of one spec, and once with every suggestion together, which reaches the two
/// refusals below that only a set can earn. So a refusal added anywhere here
/// reaches the report as well.
///
/// The grouping is load-bearing in a way a two-stub run cannot show. The
/// interpreter's import binder searches every host module carrying an import's
/// module name, so two `env` modules of one stub each bind exactly as one `env`
/// module of two. What that does not survive is the count: the store refers to
/// a host module through a `HostModuleRef`, which silently wraps an index it
/// cannot hold, and a module per stub would bind the 257th stub of one name to
/// the first stub registered and log its calls under the first one's name.
/// Grouping keeps the count to the number of module names, and a command line
/// naming more modules than a `HostModuleRef` addresses is refused rather than
/// bound to the wrong stubs.
///
/// Grouping moves the same wrap inside one module, where nothing in the
/// interpreter guards it either. Its binder records a host function's position
/// in its module as `fi as u16`, and the store never counts a module's
/// functions, so the 65,537th stub of one module name would be bound to the
/// first. No interpreter constructor judges that bound, so the refusal is
/// sized from the binder's sixteen bits rather than asked for.
fn host_stubs(
    session: &SpaceWasmSession,
    specs: &[HostSpec],
    calls: &HostCalls,
) -> Result<spacewasm::Vec<HostModule>, String> {
    let mut groups: Vec<(&HostSpec, Vec<HostFunction>)> = Vec::new();
    for spec in specs {
        let function = stub(spec, calls)?;
        match groups.iter_mut().find(|(first, _)| first.module == spec.module) {
            Some((_, functions)) => functions.push(function),
            None => groups.push((spec, vec![function])),
        }
    }
    let addressable = (0..groups.len())
        .find(|&index| usize::from(HostModuleRef::new(index).0) != index)
        .unwrap_or(groups.len());
    if addressable < groups.len() {
        return Err(format!(
            "`--host` registers stubs under {} module names, and the interpreter addresses at \
             most {addressable} host modules: the imports of the rest would be bound to \
             another module's stubs",
            groups.len()
        ));
    }
    let per_module = usize::from(u16::MAX) + 1;
    if let Some((first, functions)) =
        groups.iter().find(|(_, functions)| functions.len() > per_module)
    {
        return Err(format!(
            "`--host` registers {} stubs under `{}`, and the interpreter addresses at most \
             {per_module} functions of one host module: an import of any stub past that many \
             would be bound to one of the first",
            functions.len(),
            first.module
        ));
    }
    let modules = groups
        .into_iter()
        .map(|(first, functions)| {
            host_module(session, &first.module, functions, Vec::new()).map_err(|refusal| {
                match refusal {
                    HostSetError::Name { error, .. } => {
                        name_refusal(first, "module", &first.module, HOST_MODULE_NAME_CAP, error)
                    }
                    HostSetError::Allocation(_) => panic!("{refusal}"),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(host_set(session, modules).unwrap_or_else(|fault| panic!("{fault}")))
}

/// The stub `spec` registers: it records each call in `calls` and returns zero
/// of its result type.
///
/// # Panics
///
/// Panics if the interpreter's allocator cannot box the stub, which is a
/// harness fault as it is in [`decode_with`].
fn stub(spec: &HostSpec, calls: &HostCalls) -> Result<HostFunction, String> {
    let field = HostName::try_from_str(&spec.field).map_err(|error| {
        name_refusal(spec, "function", &spec.field, HOST_FUNCTION_NAME_CAP, error)
    })?;
    let params = HostValList::try_new(&spec.params)
        .map_err(|error| signature_refusal(spec, SignaturePart::Params, &error))?;
    let result = HostValList::try_new(&spec.result)
        .map_err(|error| signature_refusal(spec, SignaturePart::Result, &error))?;
    let zero = result.as_slice().first().map(|ty| zero_of(*ty));
    let label = format!("{}.{}", spec.module, spec.field);
    let calls = calls.clone();
    HostFunction::try_new(field, params, result, move |_: &mut Engine, args: &[Value]| {
        let args: Vec<String> = args.iter().map(|value| render(*value)).collect();
        calls.record(format!("host call: {label}({})", args.join(", ")));
        ControlFlow::Continue(zero)
    })
    .map_err(|error| signature_refusal(spec, SignaturePart::Result, &error))
}

/// Zero of `ty`, which is what every stub answers.
fn zero_of(ty: ValType) -> Value {
    match ty {
        ValType::I32 => Value::I32(0),
        ValType::I64 => Value::I64(0),
        ValType::F32 => Value::F32(0.0),
        ValType::F64 => Value::F64(0.0),
    }
}

/// A name in a `--host` spec that the interpreter will not register.
///
/// The cap is the interpreter's constant for the kind of name refused, read
/// rather than restated, so this sentence moves when the interpreter does.
fn name_refusal(
    spec: &HostSpec,
    kind: &str,
    name: &str,
    cap: usize,
    error: HostNameError,
) -> String {
    format!(
        "the interpreter refuses the {kind} name `{name}` of `--host {}` ({error:?}): a host \
         {kind} name holds at most {cap} bytes, and this one is {}",
        spec.text(),
        name.len()
    )
}

/// The half of a `--host` signature the interpreter refused.
#[derive(Clone, Copy)]
enum SignaturePart {
    Params,
    Result,
}

/// A signature the interpreter will not register, in its words and then in the
/// option's.
///
/// The interpreter spells the parameters and the result with one list type, so
/// a result of ten letters is refused as `ParameterListTooLong`; which half was
/// refused is what decides the explanation, since the cap a result exceeds is
/// the one on results.
///
/// A character that names no value type is named itself, found by putting each
/// character to the interpreter's list constructor alone: a stray second `:`,
/// which [`HostSpec::parse`] leaves in the result for exactly this refusal, or
/// one wrong letter among nine, is hard to see inside the list it sits in.
///
/// # Panics
///
/// Panics on an allocation failure, which is a harness fault rather than a
/// property of the spec.
fn signature_refusal(spec: &HostSpec, part: SignaturePart, error: &HostFunctionError) -> String {
    let (what, letters) = match part {
        SignaturePart::Params => ("parameter types", &spec.params),
        SignaturePart::Result => ("result type", &spec.result),
    };
    let why = match (error, part) {
        (HostFunctionError::ValListInvalidItem, _) => {
            let offender = match letters
                .chars()
                .find(|letter| HostValList::try_new(letter.encode_utf8(&mut [0; 4])).is_err())
            {
                Some(':') => {
                    "`:` names no value type, and a spec has at most one `:`; ".to_string()
                }
                Some(letter) => format!("`{letter}` names no value type; "),
                None => String::new(),
            };
            format!(
                "{offender}each value type is one character, `i` for `i32`, `I` for `i64`, `f` \
                 for `f32` or `d` for `f64`"
            )
        }
        (HostFunctionError::ParameterListTooLong, SignaturePart::Params) => format!(
            "a host function takes at most {MAX_HOST_FUNCTION_PARAMS} parameters, and this one \
             takes {}",
            letters.chars().count()
        ),
        (HostFunctionError::ParameterListTooLong, SignaturePart::Result)
        | (HostFunctionError::MultiReturnNotAllowed, _) => format!(
            "a host function returns at most one value, and this one returns {}",
            letters.chars().count()
        ),
        (HostFunctionError::AllocError(e), _) => {
            panic!("the interpreter's allocator cannot box a stub: {e:?}")
        }
    };
    format!(
        "the interpreter refuses the {what} `{letters}` of `--host {}` ({error:?}): {why}",
        spec.text()
    )
}

/// Reports bytes the interpreter refused, naming each import the module
/// carries and what `--host` registered for it.
///
/// The decoder's own error says that an import went unresolved, or that the
/// host function registered for it has another signature, and never which
/// import: a flight decoder keeps no string it does not have to. So the module
/// is read a second time for the one thing the verdict is missing, and each
/// import's signature is compared here against the stub registered under its
/// two names. A stub that does not match is described by the half of its
/// signature that differs and, for a parameter, by its position, which is what
/// lets a mistyped letter be read off the report instead of found by comparing
/// two spellings by eye.
///
/// Every refused module that declares an import gets the list, not only the one
/// whose verdict was about an import. Deciding otherwise would mean naming the
/// variants of the decoder's own error that mean "nothing supplied this", which
/// is a guess at somebody else's taxonomy that goes quietly wrong when they add
/// one. Each line is worded as a fact about one import rather than as a
/// diagnosis, so a reader is not told an import caused a failure the line above
/// already attributed.
///
/// A spec naming no import of the module is listed after them. It registered a
/// stub nothing binds, which is what a misspelt name does, and leaving it out
/// would send the reader to compare the list against their command line by eye
/// to find it.
///
/// Each suggestion is judged alone, and a set of stubs can be refused where
/// every one of them alone is registered: too many module names, or too many
/// stubs under one. So the suggestions are then put to [`host_stubs`] together,
/// and a refusal is printed as the reason no `--host` set can supply them all.
///
/// The closing line says what would make the module load, and one offering
/// the wrong fix is worse than none, since a reader who follows it meets a
/// second refusal before the line they needed. So it is chosen in this order:
///
/// - an import the interpreter cannot load however it is supplied ends the
///   report on that, because no SpaceWasm embedder can load the module as
///   built and any pointer would send the reader after a fix that does not
///   exist;
/// - a refusal from outside the import section, while some import is still
///   unsupplied, says that the interpreter stopped before it bound any
///   import and points nowhere: an unsupplied import would have been refused
///   inside that section, so the decoder never reached it, and a stub
///   registered for every import would meet the same verdict;
/// - a set no `--host` command line can register points at an embedder;
/// - imports of which a stub can supply some and only an embedder the rest
///   point at an embedder and say that `--host` alone is not enough;
/// - imports only an embedder can supply point at an embedder alone;
/// - imports a stub can supply point at `--host`.
///
/// A module whose every import is supplied was refused for something else and
/// gets no closing line.
///
/// It builds stubs through [`stub_for`] and [`host_stubs`], and so through the
/// interpreter's allocator, which is why it takes the session [`execute`]
/// holds.
fn report_decode_failure(
    session: &SpaceWasmSession,
    path: &Path,
    wasm: &[u8],
    error: &ParseError,
    stubs: &[HostSpec],
    err: &mut dyn Write,
) {
    line(
        err,
        &format!(
            "error: {} does not load under the SpaceWasm interpreter: {:?} at byte {}",
            path.display(),
            error.err.err,
            error.offset
        ),
    );
    if let Some(section) = error.err.section {
        line(err, &format!("  reading the {section:?} section"));
    }
    let imports = imports_of(session, wasm);
    if imports.is_empty() {
        return;
    }
    line(
        err,
        if stubs.is_empty() {
            "  it imports these, and no `--host` stub was registered for any of them:"
        } else {
            "  it imports these; beside each, what `--host` registered under its two names:"
        },
    );
    let unbindable = |why: &str, given: Option<&HostSpec>| match given {
        Some(given) => format!("the stub `--host {}` cannot bind it: {why}", given.text()),
        None => format!("no `--host` stub can bind it: {why}"),
    };
    let (mut needs_a_stub, mut needs_an_embedder, mut unloadable) = (false, false, false);
    for import in &imports {
        let given =
            stubs.iter().find(|spec| spec.names_the_same_import(&import.module, &import.field));
        let status = match (&import.stub, given) {
            (Ok(needed), Some(given)) => match mismatch(given, needed) {
                None => "stub registered".to_string(),
                Some(difference) => {
                    needs_a_stub = true;
                    format!(
                        "the stub `--host {}` {difference}; it needs `--host {}`",
                        given.text(),
                        needed.text()
                    )
                }
            },
            (Ok(needed), None) => {
                needs_a_stub = true;
                format!("no stub; it needs `--host {}`", needed.text())
            }
            (Err(Unstubbable::Names(why)), _) => {
                needs_an_embedder = true;
                format!("no `--host` spec names it: {why}")
            }
            (Err(Unstubbable::Binding(why)), given) => {
                needs_an_embedder = true;
                unbindable(why, given)
            }
            (Err(Unstubbable::Unloadable(why)), given) => {
                unloadable = true;
                unbindable(why, given)
            }
        };
        line(err, &format!("    {}: {status}", import.label()));
    }
    for stub in stubs {
        if !imports.iter().any(|import| stub.names_the_same_import(&import.module, &import.field))
        {
            line(err, &format!("    `--host {}` names no import of this module", stub.text()));
        }
    }
    let refused_as_a_set = set_refusal(session, &imports);
    if let Some(refusal) = &refused_as_a_set {
        line(err, &format!("  no `--host` set can supply them all: {refusal}"));
    }
    let embedder = "  run the module from the embedder that supplies them";
    let unsupplied = needs_a_stub || needs_an_embedder || refused_as_a_set.is_some();
    let closing = if unloadable {
        Some("  no SpaceWasm embedder can load this module as built")
    } else if unsupplied && error.err.section != Some(SectionKind::Import) {
        Some(
            "  the interpreter refused the module before it bound any import, so no stub \
             changes this verdict",
        )
    } else if refused_as_a_set.is_some() {
        Some(embedder)
    } else if needs_an_embedder && needs_a_stub {
        Some(
            "  `--host` can stub only some of these, so run the module from the embedder that \
             supplies them",
        )
    } else if needs_an_embedder {
        Some(embedder)
    } else if needs_a_stub {
        Some(
            "  register a stub for each with `--host MODULE.FIELD=PARAMS[:RESULT]`, or run the \
             module from the embedder that supplies them",
        )
    } else {
        None
    };
    if let Some(closing) = closing {
        line(err, closing);
    }
}

/// Why the specs the report suggests cannot all be registered at once, when
/// they cannot.
///
/// They are deduplicated by their two names first, as a command line has to
/// be: an import declared twice with one type binds one stub.
fn set_refusal(session: &SpaceWasmSession, imports: &[DeclaredImport]) -> Option<String> {
    let mut named = FxHashSet::default();
    let every_stub: Vec<HostSpec> = imports
        .iter()
        .filter_map(|import| import.stub.as_ref().ok())
        .filter(|spec| named.insert((spec.module.as_str(), spec.field.as_str())))
        .cloned()
        .collect();
    host_stubs(session, &every_stub, &HostCalls::default()).err()
}

/// One import a module declares, as the refusal report names it.
struct DeclaredImport {
    module: String,
    field: String,
    /// The `--host` spec that registers a stub this import binds to, or the
    /// [`Unstubbable`] reason no spec can.
    stub: Result<HostSpec, Unstubbable>,
}

impl DeclaredImport {
    /// The import's names as the report prints them: joined, as a spec writes
    /// them, unless joined they would read as another import's, and then apart.
    fn label(&self) -> String {
        match self.stub {
            Err(Unstubbable::Names(_)) => {
                format!("module `{}`, field `{}`", self.module, self.field)
            }
            Ok(_) | Err(Unstubbable::Binding(_) | Unstubbable::Unloadable(_)) => {
                format!("{}.{}", self.module, self.field)
            }
        }
    }
}

/// Why no `--host` spec registers a stub an import binds to, as the clause the
/// report prints.
///
/// Kept rather than collapsed into "no spec", because each reason has its own
/// next step, and a reader told only that nothing can be written retries the
/// obvious spelling and meets the same line again.
#[derive(Clone)]
enum Unstubbable {
    /// `MODULE.FIELD` cannot name the import: a field containing a dot, an `=`
    /// in either name, or an empty one.
    Names(String),
    /// A spec can name it, but no stub binds it: it is a table, a memory or a
    /// global, and `--host` registers functions only. An embedder can supply
    /// it, since a host module carries all three.
    Binding(String),
    /// Nothing can supply it, because the interpreter cannot load the import
    /// however it is supplied: its type holds a value type the interpreter
    /// does not have, it is a tag import, its function type cannot be read, an
    /// earlier import has the same two names and another type, or the
    /// interpreter refuses the host function it would bind to — a name longer
    /// than a host name holds, more parameters than a host function takes, or
    /// more than one result.
    ///
    /// Told apart from [`Unstubbable::Binding`] because the next step differs:
    /// an embedder is no remedy here. `--host`'s four letters are the
    /// interpreter's whole value-type set, so no embedder has a fifth to
    /// offer; the interpreter's import reader knows no tag; it binds one name
    /// to the first function registered under it, so the second of two types
    /// imported under one name is a mismatch whatever supplies it; and every
    /// embedder builds a host function through the constructors `--host`
    /// calls, so a function they refuse is one no embedder can register.
    Unloadable(String),
}

/// Why an import whose function type cannot be read has no stub. A module
/// carrying one is a module the decoder has already refused for its bytes.
fn unreadable_type() -> Unstubbable {
    Unstubbable::Unloadable("its function type cannot be read".to_string())
}

/// Every import the module declares, in import-section order, with the
/// `--host` spec each would need.
///
/// Read with the fork for convenience and not for capability: its import reader
/// yields flat imports carrying `module` and `name`, where stock `wasmparser`
/// yields encoding groups that each have to be walked to reach the same two
/// strings. It is also the reader `corpus::has_import_section` uses, so the two
/// agree about what an import section is by construction.
///
/// A parse error ends the walk, so a module malformed before its import section
/// names nothing. That is the wanted answer rather than a gap: the caller is
/// reporting bytes the interpreter has already refused, and guessing at what an
/// unreadable section might have declared would be worse than saying nothing.
fn imports_of(session: &SpaceWasmSession, wasm: &[u8]) -> Vec<DeclaredImport> {
    let mut signatures: Vec<Result<String, Unstubbable>> = Vec::new();
    let mut imports = Vec::new();
    for payload in inf_wasmparser::Parser::new(0).parse_all(wasm).flatten() {
        match payload {
            inf_wasmparser::Payload::TypeSection(section) => {
                signatures = section
                    .into_iter_err_on_gc_types()
                    .map(|ty| ty.map_err(|_| unreadable_type()).and_then(|ty| host_signature(&ty)))
                    .collect();
            }
            inf_wasmparser::Payload::ImportSection(section) => {
                for import in section.into_iter().flatten() {
                    imports.push(DeclaredImport {
                        module: import.module.to_string(),
                        field: import.name.to_string(),
                        stub: stub_for(session, &import, &signatures),
                    });
                }
            }
            _ => {}
        }
    }
    refuse_names_imported_again(&mut imports);
    imports
}

/// Marks each import that repeats an earlier import's two names with another
/// function type as one nothing can supply, for the reason
/// [`Unstubbable::Unloadable`] gives.
///
/// No spec is suggested for it: it would be a second spec for the same two
/// names, which `--host` refuses as given twice. The same name imported again
/// with the same type binds the same stub, and is left alone.
fn refuse_names_imported_again(imports: &mut [DeclaredImport]) {
    let mut first_signature: FxHashMap<(String, String), String> = FxHashMap::default();
    for import in imports {
        let Ok(spec) = &import.stub else {
            continue;
        };
        let signature = signature_text(&spec.params, &spec.result);
        let first = first_signature
            .entry((import.module.clone(), import.field.clone()))
            .or_insert_with(|| signature.clone());
        if *first != signature {
            import.stub = Err(Unstubbable::Unloadable(
                "an earlier import has the same two names and another type, and the interpreter \
                 binds one name to one function"
                    .to_string(),
            ));
        }
    }
}

/// The spec that registers a stub `import` binds to, or why none can, given
/// the module's function types in `--host`'s spelling.
///
/// A name containing `=` is refused before any spec is spelled, because the
/// option splits a spec at its first `=`, so the spelling would be read back
/// as other names and the reason would come out as a complaint about them.
/// Any other spec is read back through [`HostSpec::parse`] rather than
/// trusted: one that does not come back naming the same two names is a spec
/// `--host` cannot write, and printing it would point the reader at a stub
/// registered under names the import does not carry. It is then put to
/// [`host_stubs`], the very function a command line meets, so a spec the
/// report prints is one the interpreter registers on its own: the interpreter
/// decodes imports it can never bind — a 32-byte name, a tenth parameter — and
/// a spec for one of those would be refused the moment the reader pasted it.
/// Such an import is [`Unstubbable::Unloadable`], since the refusal comes from
/// the host-function constructors every embedder builds its hosts through.
fn stub_for(
    session: &SpaceWasmSession,
    import: &inf_wasmparser::Import<'_>,
    signatures: &[Result<String, Unstubbable>],
) -> Result<HostSpec, Unstubbable> {
    let not_a_function = |kind: &str| {
        Unstubbable::Binding(format!(
            "it is a {kind} import, and `--host` registers functions only"
        ))
    };
    let signature = match import.ty {
        inf_wasmparser::TypeRef::Func(ty) => {
            signatures.get(ty as usize).cloned().unwrap_or_else(|| Err(unreadable_type()))
        }
        inf_wasmparser::TypeRef::Table(_) => Err(not_a_function("table")),
        inf_wasmparser::TypeRef::Memory(_) => Err(not_a_function("memory")),
        inf_wasmparser::TypeRef::Global(_) => Err(not_a_function("global")),
        inf_wasmparser::TypeRef::Tag(_) => Err(Unstubbable::Unloadable(
            "it is a tag import, which the SpaceWasm interpreter does not support".to_string(),
        )),
    }?;
    for (kind, name) in [("module", import.module), ("field", import.name)] {
        if name.contains('=') {
            return Err(Unstubbable::Names(format!(
                "its {kind} name `{name}` contains `=`, which ends a name in `--host`'s spelling"
            )));
        }
    }
    let text = format!("{}.{}={signature}", import.module, import.name);
    let spec = HostSpec::parse(&text).map_err(Unstubbable::Names)?;
    if !spec.names_the_same_import(import.module, import.name) {
        return Err(Unstubbable::Names(format!(
            "`--host {text}` would register the field `{}` of the module `{}`",
            spec.field, spec.module
        )));
    }
    host_stubs(session, std::slice::from_ref(&spec), &HostCalls::default())
        .map_err(Unstubbable::Unloadable)?;
    Ok(spec)
}

/// A function type in `--host`'s spelling, `PARAMS[:RESULT]`, or, naming the
/// first of its value types that alphabet has no letter for, why nothing can
/// supply it.
fn host_signature(ty: &inf_wasmparser::FuncType) -> Result<String, Unstubbable> {
    let letters = |types: &[inf_wasmparser::ValType]| -> Result<String, Unstubbable> {
        types
            .iter()
            .map(|ty| match ty {
                inf_wasmparser::ValType::I32 => Ok('i'),
                inf_wasmparser::ValType::I64 => Ok('I'),
                inf_wasmparser::ValType::F32 => Ok('f'),
                inf_wasmparser::ValType::F64 => Ok('d'),
                inf_wasmparser::ValType::V128 | inf_wasmparser::ValType::Ref(_) => {
                    Err(Unstubbable::Unloadable(format!(
                        "its type holds `{ty}`, a value type the SpaceWasm interpreter does not \
                         have"
                    )))
                }
            })
            .collect()
    };
    Ok(signature_text(&letters(ty.params())?, &letters(ty.results())?))
}

/// What the stub `given` does differently from the stub `needed`, a clause for
/// each half of the signature that differs, or `None` when the two match.
///
/// A parameter list of the right length is described by every position that
/// differs, a clause for each pair of types, and one of the wrong length by
/// the two counts, since a position means nothing once the lists no longer
/// line up. Every position rather than the first, because a clause naming one
/// reads as the whole difference, and a reader who fixed that one letter would
/// meet a second report about the next.
fn mismatch(given: &HostSpec, needed: &HostSpec) -> Option<String> {
    let mut clauses = Vec::new();
    let (given_params, needed_params) = (value_types(&given.params), value_types(&needed.params));
    if given_params.len() != needed_params.len() {
        clauses.push(format!(
            "takes {} where the import takes {}",
            parameters_phrase(given_params.len()),
            parameters_phrase(needed_params.len())
        ));
    } else {
        let mut differing: Vec<((ValType, ValType), Vec<usize>)> = Vec::new();
        let pairs = given_params.iter().copied().zip(needed_params.iter().copied());
        for (index, pair) in pairs.enumerate().filter(|(_, (stub, import))| stub != import) {
            match differing.iter_mut().find(|(seen, _)| *seen == pair) {
                Some((_, positions)) => positions.push(index),
                None => differing.push((pair, vec![index])),
            }
        }
        for ((given_ty, needed_ty), positions) in differing {
            clauses.push(format!(
                "takes `{}` as its {} where the import takes `{}`",
                type_name(given_ty),
                positions_phrase(&positions),
                type_name(needed_ty)
            ));
        }
    }
    let (given_result, needed_result) = (value_types(&given.result), value_types(&needed.result));
    if given_result != needed_result {
        clauses.push(format!(
            "returns {} where the import returns {}",
            results_phrase(&given_result),
            results_phrase(&needed_result)
        ));
    }
    if clauses.is_empty() { None } else { Some(clauses.join(", and ")) }
}

/// The value types a run of `--host` letters spells, as the interpreter's own
/// list constructor reads them.
///
/// # Panics
///
/// Panics if the interpreter refuses the letters, which [`mismatch`] passes it
/// only after the interpreter has built a stub from them.
fn value_types(letters: &str) -> Vec<ValType> {
    HostValList::try_new(letters)
        .expect("the interpreter has already built a stub from these letters")
        .as_slice()
        .to_vec()
}

/// "first" to "ninth", then "10th" and on, for a parameter's position counted
/// from zero.
fn ordinal(index: usize) -> String {
    const WORDS: [&str; 9] =
        ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth"];
    WORDS.get(index).map_or_else(|| format!("{}th", index + 1), |word| (*word).to_string())
}

/// "first parameter", "first and third parameters" or "first, second and
/// fourth parameters", for positions counted from zero.
///
/// # Panics
///
/// Panics on no positions at all, which [`mismatch`] never passes: it groups
/// the positions that differ by their pair of types, so each group holds at
/// least the one that started it.
fn positions_phrase(positions: &[usize]) -> String {
    let words: Vec<String> = positions.iter().map(|&index| ordinal(index)).collect();
    match words.as_slice() {
        [only] => format!("{only} parameter"),
        [earlier @ .., last] => format!("{} and {last} parameters", earlier.join(", ")),
        [] => unreachable!("a pair of types that differs differs somewhere"),
    }
}

/// "none", "1 parameter" or "3 parameters".
fn parameters_phrase(count: usize) -> String {
    match count {
        0 => "none".to_string(),
        1 => "1 parameter".to_string(),
        count => format!("{count} parameters"),
    }
}

/// "nothing", or the result types in backticks.
fn results_phrase(types: &[ValType]) -> String {
    if types.is_empty() {
        return "nothing".to_string();
    }
    let names: Vec<String> = types.iter().map(|ty| format!("`{}`", type_name(*ty))).collect();
    names.join(" and ")
}

/// One export as a signature a reader can copy back onto the command line.
fn describe(function: &ExportedFunction) -> String {
    let params: Vec<&str> = function.params.iter().map(|ty| type_name(*ty)).collect();
    let result = match function.result {
        Some(ty) => format!(" -> {}", type_name(ty)),
        None => String::new(),
    };
    format!("{}({}){result}", function.name, params.join(", "))
}

/// The measurement as a person reads it.
///
/// The words line carries the capacity and the percentage as well, which is the
/// shape `spacewasm_std` prints its code-word usage in.
fn human_lines(stats: &IrStats) -> Vec<String> {
    let capacity = stats.ir_words_capacity();
    let used = if capacity == 0 {
        0.0
    } else {
        100.0 * stats.ir_words as f64 / capacity as f64
    };
    vec![
        format!("code pages: {}", stats.code_pages),
        format!("IR words (16-bit): {} / {capacity} ({used:.2}%)", stats.ir_words),
        format!("IR bytes: {}", stats.ir_bytes()),
        format!("wasm bytes: {}", stats.wasm_bytes),
        format!("IR bytes per wasm byte: {:.2}", stats.ir_bytes_per_wasm_byte()),
    ]
}

/// The measurement as one line of JSON.
///
/// Hand-assembled rather than serialized: every value is a number, so there is
/// nothing to escape and the key order in this function is the documented key
/// order. The ratio is written at full precision rather than at the two
/// decimals the human rendering shows, and it is a finite number to write:
/// only a module that decoded is measured, and such a module carried at least
/// a magic number and a version, so the divisor is never zero.
fn json_line(stats: &IrStats) -> String {
    format!(
        "{{\"code_pages\":{},\"ir_words\":{},\"ir_bytes\":{},\"wasm_bytes\":{},\
         \"ir_bytes_per_wasm_byte\":{}}}",
        stats.code_pages,
        stats.ir_words,
        stats.ir_bytes(),
        stats.wasm_bytes,
        stats.ir_bytes_per_wasm_byte()
    )
}
