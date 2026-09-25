//! Harness for driving this compiler's output against the real SpaceWasm
//! interpreter, in process.
//!
//! SpaceWasm is a `no_std` flight interpreter: it decodes, validates and
//! compiles a WebAssembly 1.0 module to its own 16-bit IR, then runs that IR
//! against a fuel budget. Decoding *is* validation *is* IR compilation — there
//! is no validate-only entry point — so [`decode`] returning `Ok` is the whole
//! statement "this artifact loads on the target runtime", and a [`ParseError`]
//! carries both the offset and the reason a flight computer would have given.
//!
//! # The single-threaded contract, and why this tier is stricter than its sibling
//!
//! `spacewasm::global_allocator!` expands to two `static mut` globals and the
//! `#[no_mangle] extern "C"` symbols the library's internal allocations resolve
//! to. Upstream documents those entry points as single-threaded and
//! non-re-entrant, and puts the synchronization on the embedder: reaching them
//! from more than one thread is undefined behaviour by that contract, and
//! `cargo test` runs a binary's tests on many threads at once. The token below
//! is that synchronization.
//!
//! The contract is the reason rather than an observed write, and deliberately
//! so. [`StdAllocator`] is zero-sized, so today neither static is written after
//! its initializer and two threads decoding at once would only read them. That
//! is what makes the token look like ceremony to a reader who checks — and it
//! stops being one the moment the allocator carries state, which the bounded
//! page allocator a flight embedder actually ships would give it.
//!
//! [`SpaceWasmSession`] is the answer, and it is deliberately stricter than the
//! Soroban tier's plain `session()` guard next door. There, a forgotten lock
//! costs a flaky host; here it puts the harness outside the library's stated
//! contract, so for loading, running and freeing a module the lock is not a
//! convention a test is asked to remember: it is a token no test can forge, and
//! the only route to a loaded module is [`SpaceWasmSession::acquire`] followed
//! by [`decode`]. A [`LoadedModule`] additionally *borrows* the session it was
//! decoded under, so a module cannot outlive the lock that protects the
//! allocator it will be freed through — the case a bare token would still let a
//! test write.
//!
//! One route is guarded by convention alone: building a host value. A host
//! function boxes its closure and a host module or a host set is a list, all
//! allocated through the same allocator, yet [`host_module`], [`host_set`] and
//! the interpreter's own `HostFunction::try_new` take no token. So a caller
//! builds its hosts after `acquire` and moves them straight into
//! [`decode_with`], as every caller in this tier does. No token is threaded
//! through the two builders because one could not close the route:
//! spacewasm's own public constructors, `HostFunction::try_new` and
//! `spacewasm::Vec::from_exact_iter` among them, allocate, and no signature in
//! this file stops a test calling them. It would also cost the oracle its
//! inline host sets: a host set built as an argument beside the `&mut` session
//! the decode takes would borrow the session twice in one call.
//!
//! The macro is invoked at each **binary root** and never here, so the `static
//! mut` cannot reach this crate's library or its `rocq-discharge` binaries.
//! Three roots invoke it: the corpus-sweep binary this file sits beside, the
//! `spacewasm-embed` example, and the CLI matrix that drives that example's
//! entry point in process.
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
//! unbounded [`StdAllocator`] this tier needs and that allocator keeps no
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

use std::alloc::Layout;
use std::cell::RefCell;
use std::io::Write;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rustc_hash::{FxHashMap, FxHashSet};
use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, Engine, ExportDesc,
    HOST_FUNCTION_NAME_CAP, HOST_MODULE_NAME_CAP, HostFunction, HostFunctionError, HostGlobal,
    HostModule, HostModuleRef, HostName, HostNameError, HostValList, InnerVec, Interpreter,
    InterpreterResult, InterpreterRunner, InvokeError, MAX_HOST_FUNCTION_PARAMS, Module, ModuleRef,
    ParseError, Ref, SectionKind, TrapReason, ValType, Value, WasmMemoryAllocator, WasmRef,
    WasmStream,
};

/// Control frames the reference embedder's verifier admits.
///
/// `spacewasm_std`, upstream's own `std` embedding, is the configuration this
/// tier reports against: quoting a number nobody ships would make every depth
/// finding unactionable.
pub const EMBEDDER_MAX_CONTROL_FRAMES: usize = 64;

/// Operand-stack depth the reference embedder's verifier admits. See
/// [`EMBEDDER_MAX_CONTROL_FRAMES`].
pub const EMBEDDER_MAX_STACK_DEPTH: usize = 256;

/// IR pages the code builder may fill, in `spacewasm_std`'s configuration.
///
/// A page holds 256 sixteen-bit words, so this is 128 KiB of compiled IR. A
/// module needing more fails to load rather than silently growing the budget,
/// which is the honest reading for a target whose whole premise is a fixed
/// memory envelope.
const MAX_CODE_PAGES: usize = 256;

/// Words the interpreter's value stack holds.
///
/// Larger than `spacewasm_std`'s 1024 because this tier runs the whole codegen
/// corpus rather than one hand-picked program, and a stack bound reached by a
/// recursive fixture would be reported as a trap — a difference from `wasmtime`
/// that says nothing about either engine's semantics.
const STACK_WORDS: usize = 64 * 1024;

/// Modules one engine may hold. Each [`LoadedModule`] builds its own engine, so
/// one is all a load ever needs.
const MAX_MODULES: usize = 1;

/// Sixteen-bit words one IR page holds.
const WORDS_PER_PAGE: usize = 256;

/// The instruction budget every invocation in this tier runs under.
///
/// Large enough that no corpus fixture reaches it — an [`Outcome::OutOfFuel`]
/// is reported as a failure naming the function rather than tolerated — and
/// finite so a lowering bug that produces an unterminated loop fails the suite
/// instead of hanging it.
pub const FUEL: usize = 100_000_000;

// ---------------------------------------------------------------------------
// The allocator
// ---------------------------------------------------------------------------

/// The interpreter's allocator, backed by the Rust global allocator.
///
/// Unbounded on purpose: `spacewasm_std` runs behind a 16 × 8 KiB
/// `PageAllocator` because a flight computer has that much and no more, and the
/// same allocator here would starve on the larger corpus modules and report an
/// allocation failure where this tier means to report a decode verdict. What
/// the bounded configuration costs is a conformance measurement of its own, not
/// a verdict about whether an artifact loads, so it is not this tier's to state.
///
/// One type serves both allocator traits, as upstream's `RustSystemAllocator`
/// does: [`Allocator`] for the interpreter's own structures, and
/// [`WasmMemoryAllocator`] for a module's guest linear memory.
pub struct StdAllocator;

/// The pointer a zero-sized request is answered with.
///
/// `std::alloc::alloc` is undefined behaviour on a zero-sized layout, and both
/// of the allocator traits below can be reached with one — a module declaring
/// `(memory 0)` asks for a guest memory of no bytes. Upstream's implementations
/// pass the layout straight through and lean on the trait's "caller guarantees
/// non-zero" clause; answering with a dangling but correctly aligned pointer
/// costs two branches and removes the question, since a zero-sized allocation
/// is never read or written.
fn dangling_for(layout: Layout) -> *mut u8 {
    std::ptr::without_provenance_mut(layout.align())
}

// SAFETY: every `Ok` is either a live `std::alloc` allocation made with the
// requested layout, or — for a zero-sized layout, which is never dereferenced —
// a correctly aligned dangling pointer. `dealloc` releases exactly what `alloc`
// returned, under the same layout, and skips the zero-sized case that owns
// nothing.
unsafe impl Allocator for StdAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        if layout.size() == 0 {
            return Ok(dangling_for(layout));
        }
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            Err(AllocError::AllocationFailed)
        } else {
            Ok(ptr)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        unsafe { std::alloc::dealloc(ptr, layout) }
    }
}

impl WasmMemoryAllocator for StdAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        if layout.size() == 0 {
            return NonNull::new(dangling_for(layout)).ok_or(AllocError::AllocationFailed);
        }
        // SAFETY: the layout is non-zero-sized, which is `std::alloc::alloc`'s
        // only requirement.
        NonNull::new(unsafe { std::alloc::alloc(layout) }).ok_or(AllocError::AllocationFailed)
    }

    fn reallocate(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        if old_layout.size() == 0 {
            return self.allocate(layout);
        }
        if layout.size() == 0 {
            self.deallocate(ptr, old_layout);
            return NonNull::new(dangling_for(layout)).ok_or(AllocError::AllocationFailed);
        }
        // SAFETY: `ptr` came from `allocate`/`reallocate` under `old_layout`,
        // both sizes are non-zero, and `realloc` leaves the original block
        // untouched when it returns null — which is the failure contract this
        // trait states.
        NonNull::new(unsafe { std::alloc::realloc(ptr.as_ptr(), old_layout, layout.size()) })
            .ok_or(AllocError::AllocationFailed)
    }

    fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        // SAFETY: `ptr` was returned by `allocate`/`reallocate` under `layout`.
        unsafe { std::alloc::dealloc(ptr.as_ptr(), layout) }
    }
}

// ---------------------------------------------------------------------------
// The session token
// ---------------------------------------------------------------------------

/// Exclusive access to the interpreter for the duration of one test.
///
/// Holding one is what makes reaching the interpreter sound: see this module's
/// documentation for why a convention would not be enough here.
pub struct SpaceWasmSession {
    /// Held for the token's lifetime and never read: it is the lock itself.
    _guard: MutexGuard<'static, ()>,
}

impl SpaceWasmSession {
    /// Blocks until this binary's interpreter is free, then claims it.
    ///
    /// Acquire exactly once per test and pass the `&mut SpaceWasmSession` down:
    /// the lock is a plain non-reentrant [`Mutex`], so a second acquire on the
    /// same thread deadlocks the binary with no output at all — the one failure
    /// mode of this design that does not name itself.
    ///
    /// The guard never poisons: a panicking assertion in one test would
    /// otherwise turn every later `lock()` into an error and bury the one real
    /// failure under a wall of secondary ones — and a corpus sweep would report
    /// a hundred of them. This is the Soroban tier's acquire line verbatim, for
    /// the same reason.
    #[must_use]
    pub fn acquire() -> Self {
        static SESSION: Mutex<()> = Mutex::new(());
        Self { _guard: SESSION.lock().unwrap_or_else(PoisonError::into_inner) }
    }
}

// ---------------------------------------------------------------------------
// The stream
// ---------------------------------------------------------------------------

/// A [`WasmStream`] over one in-memory module.
///
/// The decoder pulls its input in chunks and hands each one back through
/// [`WasmStream::return_`] so a flight embedder can reuse a fixed buffer. This
/// one owns a single chunk holding the whole module and lends it out once: the
/// buffer is never transferred, so the hand-back is a no-op and nothing is
/// leaked or double-freed.
pub struct ByteStream {
    buffer: Vec<u8>,
    lent: bool,
}

impl ByteStream {
    /// Wraps a copy of `wasm`.
    #[must_use]
    pub fn new(wasm: &[u8]) -> Self {
        Self { buffer: wasm.to_vec(), lent: false }
    }
}

impl WasmStream for ByteStream {
    fn read(&mut self) -> Result<Option<InnerVec<u8>>, u8> {
        if self.lent {
            return Ok(None);
        }
        self.lent = true;
        let len = self.buffer.len();
        // SAFETY: the pointer and length describe `self.buffer`, which outlives
        // every chunk because the decoder returns each one before `Module::new`
        // returns and this stream owns the allocation throughout. Capacity is
        // reported as the length so nothing beyond the module's own bytes is
        // ever addressable through the chunk.
        Ok(Some(unsafe {
            InnerVec::from_raw_parts(self.buffer.as_mut_ptr(), len, len)
        }))
    }

    fn return_(&mut self, _chunk: InnerVec<u8>) {
        // Nothing to reclaim, and nothing this body could reclaim: an
        // `InnerVec` owns no allocation and has no `Drop` of its own, so the
        // chunk falling out of scope frees nothing. The buffer it views belongs
        // to `self` and stays alive for the whole decode.
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// A module decoded, IR-compiled and instantiated inside its own engine.
///
/// Named for what it is rather than `Decoded`, which the Soroban tier next door
/// already uses for a decoded `Val`.
///
/// The `'session` lifetime is the safety argument: the engine, its IR pages and
/// the guest memory are all freed through the process-wide interpreter
/// allocator, so this value may not outlive the lock that serializes it.
pub struct LoadedModule<'session> {
    engine: Engine,
    code_builder: CodeBuilder,
    module: ModuleRef,
    session: PhantomData<&'session mut SpaceWasmSession>,
}

/// One exported function, as the decoder sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedFunction {
    /// The name in the export section.
    pub name: String,
    /// What the *WebAssembly* signature takes, in order. A hidden pointer the
    /// lowering introduced for an aggregate return is one of them, which is why
    /// this is read here and not off the source-level export descriptor — and
    /// the types rather than a count, because a caller supplying arguments on a
    /// command line has to know which of them the engine wants 64 bits wide.
    pub params: Vec<ValType>,
    /// What it gives back, if anything.
    pub result: Option<ValType>,
}

/// One export that is a host import the module exports again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReexportedImport {
    /// The name in the export section.
    pub name: String,
    /// The host import behind it, as `module.field`.
    pub import: String,
}

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
/// # Errors
///
/// Returns the decoder's own [`ParseError`], carrying the byte offset and the
/// validation reason.
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
/// for the callers that register one: the harness's `--host` stubs, the
/// host-imports tier, which runs a compiled program against recording hosts,
/// and the oracle rows whose module declares an import. Built with
/// [`host_module`] and [`host_set`].
///
/// The session is never read: it is taken by mutable reference so that the
/// returned module borrows it, which is what forbids a module outliving the
/// lock its memory is freed under.
///
/// # Errors
///
/// Returns the decoder's own [`ParseError`].
///
/// # Panics
///
/// Panics if the interpreter cannot be built at all — an allocation failure or
/// an oversized code-page budget — since that is a harness fault rather than a
/// verdict about `wasm`.
pub fn decode_with<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
) -> Result<LoadedModule<'s>, ParseError> {
    decode_with_pages::<CONTROL_FRAMES, STACK_DEPTH>(session, wasm, hosts, MAX_CODE_PAGES)
}

/// [`decode_with`] with the code builder's IR page budget chosen by the caller
/// as well.
///
/// `MAX_CODE_PAGES` is `spacewasm_std`'s, and it is the right budget for
/// every question about an artifact this compiler could write. It is the wrong
/// one for a module built to sit past an index the interpreter narrows: that
/// needs more than 65,536 function bodies, which is more compiled IR than a
/// flight configuration holds, and a row decoding it under the reference budget
/// would record a page-budget refusal while saying nothing about the index.
///
/// The budget is the embedder's own choice — `CompilerOptions::max_code_pages`
/// is what a mission integrator sets — so widening it is a parameter here
/// rather than a second allocator or a second session token: the allocator
/// contract and the single-threaded lock are untouched, and a caller passing a
/// larger number is describing a larger flight computer, not evading anything.
///
/// The module is decoded under the empty name. A named module shares the
/// namespace the host modules are registered in, and the interpreter refuses
/// one whose name a host module already carries as `DuplicateModuleName`
/// before it reads a section, so a guest named `main` would fail at byte 8
/// beside the host module `--host main.f=…` registers. The empty name is
/// exempt from that check, and nothing needs the guest's name: an engine here
/// holds this one module, so no other module imports from it.
///
/// # Errors
///
/// Returns the decoder's own [`ParseError`].
///
/// # Panics
///
/// Panics if the interpreter cannot be built at all, as [`decode_with`] does.
pub fn decode_with_pages<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    _session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
    max_code_pages: usize,
) -> Result<LoadedModule<'s>, ParseError> {
    let mut code_builder = CodeBuilder::new(CompilerOptions {
        allow_memory_grow: false,
        max_backpatch_iterations: None,
        max_code_pages,
    })
    .expect("the code builder's page budget is allocatable");
    let mut engine = Engine::new(STACK_WORDS, MAX_MODULES, hosts).expect("the engine allocates");

    let mut stream = ByteStream::new(wasm);
    let module = Module::new::<CONTROL_FRAMES, STACK_DEPTH>(
        "",
        &mut stream,
        &mut engine.store,
        &mut code_builder,
        spacewasm::Rc::new(StdAllocator)
            .expect("the guest memory allocator allocates")
            .into_wasm_memory_allocator(),
    )?;
    let module = engine.push_module(module).expect("one module fits in the store");

    let mut loaded =
        LoadedModule { engine, code_builder, module, session: PhantomData };
    loaded.run_start();
    Ok(loaded)
}

impl LoadedModule<'_> {
    /// Runs the module's start function, if it declares one.
    ///
    /// # Panics
    ///
    /// Panics if the start function does not complete. A start function that
    /// traps leaves a module no embedder would use, and this compiler emits
    /// none at all, so it is a harness surprise rather than a comparable
    /// outcome.
    fn run_start(&mut self) {
        let Some(start) = self.engine.module_start(self.module) else {
            return;
        };
        self.engine.invoke(start, &[]).expect("the start function is invocable");
        let outcome = Interpreter.run(self.code_builder.pages(), &mut self.engine, FUEL);
        assert_eq!(
            outcome,
            InterpreterResult::Finished,
            "the start function did not complete: {outcome:?}"
        );
    }

    /// Every function this module exports, in export-section order.
    ///
    /// A host import the module exports again is left out, because the engine
    /// invokes only WebAssembly functions and a host function has no body here
    /// to run; [`LoadedModule::exported_host_imports`] lists those.
    ///
    /// # Panics
    ///
    /// Panics if an exported function index does not resolve, which is a
    /// decoder invariant rather than a property of the module under test.
    #[must_use]
    pub fn exported_functions(&self) -> Vec<ExportedFunction> {
        let module = &self.engine.store.modules()[self.module.0 as usize];
        module
            .exports
            .iter()
            .filter_map(|export| {
                let ExportDesc::Func(index) = export.desc else {
                    return None;
                };
                let reference = module
                    .get_func_ref(index)
                    .expect("an exported function index resolves");
                let (owner, local) = match reference {
                    Ref::Module(local) => (self.module, local),
                    Ref::Extern { module, index } => (module, index),
                    Ref::Host { .. } => return None,
                };
                let owner = &self.engine.store.modules()[owner.0 as usize];
                let function = &owner.functions[local as usize];
                Some(ExportedFunction {
                    name: export.name.to_string(),
                    params: owner.types[function.ty.0 as usize].params.iter().copied().collect(),
                    result: function.return_ty,
                })
            })
            .collect()
    }

    /// Calls `export` with `args` under a `fuel`-instruction budget.
    ///
    /// # Panics
    ///
    /// Panics if `export` is not an exported function of this module, if it
    /// resolves to a host import rather than a function with a WebAssembly
    /// body, if the argument list does not match its signature, or if a
    /// registered host function pauses the call — all harness faults, not
    /// program outcomes. A pause is a host asking for asynchronous work, which
    /// only an embedder that resumes the call can honour, and this harness
    /// never does.
    pub fn invoke(&mut self, export: &str, args: &[Value], fuel: usize) -> Outcome {
        let reference = self.func_ref(export);
        match self.engine.invoke(reference, args) {
            Ok(()) => {}
            Err(InvokeError::StackOverflow) => return Outcome::Trap(TrapReason::StackOverflow),
            Err(e) => panic!("`{export}` could not be invoked: {e:?}"),
        }
        let result_ty = {
            let owner = &self.engine.store.modules()[reference.module.0 as usize];
            owner.functions[reference.index as usize].return_ty
        };
        match Interpreter.run(self.code_builder.pages(), &mut self.engine, fuel) {
            InterpreterResult::Finished => Outcome::Value(
                result_ty.map(|ty| {
                    self.engine.result.expect("a function with a result leaves one").to_value(ty)
                }),
            ),
            InterpreterResult::Trap(reason) => Outcome::Trap(reason),
            InterpreterResult::OutOfFuel => Outcome::OutOfFuel,
            InterpreterResult::Pause => panic!(
                "`{export}` paused: a registered host function returned \
                 `HostFunctionBreak::Pause`, and this harness never resumes a paused call"
            ),
        }
    }

    /// Every host import this module exports again, in export-section order:
    /// the exports [`LoadedModule::exported_functions`] leaves out.
    ///
    /// This is how the harness tells a reader what those names are, both where
    /// it lists the exports and where one is asked for by name — without it,
    /// the export section would read as shorter than it is.
    ///
    /// Only the command line asks this, so in the sweep binary it is kept live
    /// by the `allow` on [`run`] rather than by a caller.
    #[must_use]
    pub fn exported_host_imports(&self) -> Vec<ReexportedImport> {
        let module = &self.engine.store.modules()[self.module.0 as usize];
        module
            .exports
            .iter()
            .filter_map(|export| {
                let ExportDesc::Func(index) = export.desc else {
                    return None;
                };
                match module.get_func_ref(index)? {
                    Ref::Host { module, index } => Some(ReexportedImport {
                        name: export.name.to_string(),
                        import: self.host_function_name(module, index),
                    }),
                    Ref::Module(_) | Ref::Extern { .. } => None,
                }
            })
            .collect()
    }

    /// The `module.field` a host function was registered under.
    fn host_function_name(&self, module: HostModuleRef, index: u16) -> String {
        let host = &self.engine.store.host_modules()[usize::from(module.0)];
        format!("{}.{}", host.name.as_str(), host.functions[usize::from(index)].name())
    }

    /// Resolves an export name to the reference the engine invokes through.
    fn func_ref(&self, export: &str) -> WasmRef {
        let module = &self.engine.store.modules()[self.module.0 as usize];
        let entry = module
            .exports
            .iter()
            .find(|entry| entry.name == export)
            .unwrap_or_else(|| panic!("this module exports no `{export}`"));
        let ExportDesc::Func(index) = entry.desc else {
            panic!("`{export}` is exported, but not as a function")
        };
        match module.get_func_ref(index).expect("an exported function index resolves") {
            Ref::Module(local) => WasmRef { module: self.module, index: local },
            Ref::Extern { module, index } => WasmRef { module, index },
            Ref::Host { module, index } => {
                panic!("{}", host_export_refusal(export, &self.host_function_name(module, index)))
            }
        }
    }
}

/// Why an export naming a host import cannot be invoked, worded once for the
/// command line and for an in-process caller alike.
///
/// The sentence carries its reason and its remedy rather than leaving them to
/// this file: a reader who has just registered the import has no cause to
/// expect it to be uncallable, and nothing in either caller's context says
/// what to invoke instead.
fn host_export_refusal(export: &str, import: &str) -> String {
    format!(
        "`{export}` resolves to the host import `{import}`: the interpreter invokes only \
         functions with a WebAssembly body, and a host import's body is the embedder's, so \
         invoke an export that calls it instead"
    )
}

/// How much IR one module compiled to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrStats {
    /// Pages the code builder filled. Each holds 256 sixteen-bit words.
    pub code_pages: usize,
    /// Sixteen-bit IR words written, across every page.
    pub ir_words: usize,
    /// Bytes of WebAssembly the IR was compiled from.
    pub wasm_bytes: usize,
}

impl IrStats {
    /// The IR as bytes rather than sixteen-bit words.
    #[must_use]
    pub fn ir_bytes(self) -> usize {
        self.ir_words * BYTES_PER_WORD
    }

    /// Words the pages held could have taken, which is what the words written
    /// are usefully read against.
    #[must_use]
    pub fn ir_words_capacity(self) -> usize {
        self.code_pages * WORDS_PER_PAGE
    }

    /// Bytes of IR per byte of WebAssembly.
    ///
    /// Only ever computed for a module that decoded, and a module that decoded
    /// carried at least a magic number and a version, so the divisor is never
    /// zero.
    #[must_use]
    pub fn ir_bytes_per_wasm_byte(self) -> f64 {
        self.ir_bytes() as f64 / self.wasm_bytes as f64
    }
}

/// Measures the IR `module` compiled to, against the `wasm_len` bytes it came
/// from.
#[must_use]
pub fn ir_stats(module: &LoadedModule<'_>, wasm_len: usize) -> IrStats {
    let pages = module.code_builder.pages().len();
    // Every page but the last is full; the builder's offset is how far into the
    // last one the writer got.
    let filled = pages.saturating_sub(1) * WORDS_PER_PAGE + module.code_builder.offset();
    IrStats { code_pages: pages, ir_words: filled, wasm_bytes: wasm_len }
}

// ---------------------------------------------------------------------------
// Host modules
// ---------------------------------------------------------------------------

/// The host module an embedder registers under `name`, supplying `functions`
/// and `globals` to the modules that import them.
///
/// Every host module in this tier is built here — the oracle's one-function
/// and one-global registrations, the host-imports tier's recording hosts and
/// the harness's `--host` stubs — so the two lists no caller fills, memories
/// and tables, are left empty in one place rather than in each.
///
/// The name is handed to the interpreter's own constructor and its refusal is
/// returned rather than unwrapped: `--host` passes a name somebody typed, and
/// one that is too long is a usage error to report rather than a harness fault.
///
/// It allocates through the interpreter's allocator and takes no session token;
/// this module's documentation says why that is the one route left to
/// convention, and what every caller does instead.
///
/// # Errors
///
/// Returns the interpreter's [`HostNameError`] when `name` is longer than a
/// host module name holds.
///
/// # Panics
///
/// Panics if the interpreter's allocator cannot hold the two lists, which is a
/// harness fault as it is in [`decode_with`].
pub fn host_module(
    name: &str,
    functions: Vec<HostFunction>,
    globals: Vec<HostGlobal>,
) -> Result<HostModule, HostNameError> {
    Ok(HostModule {
        name: HostName::try_from_str(name)?,
        globals: interpreter_vec(globals),
        functions: interpreter_vec(functions),
        memory: spacewasm::Vec::zero(),
        table: spacewasm::Vec::zero(),
    })
}

/// `modules` as the host set [`decode_with`] takes.
///
/// # Panics
///
/// Panics if the interpreter's allocator cannot hold the list, as
/// [`host_module`] does.
#[must_use]
pub fn host_set(modules: Vec<HostModule>) -> spacewasm::Vec<HostModule> {
    interpreter_vec(modules)
}

/// `items`, moved into a list allocated by the interpreter's allocator, which
/// is the only kind of list a host module or a host set is made of.
fn interpreter_vec<T>(items: Vec<T>) -> spacewasm::Vec<T> {
    spacewasm::Vec::from_exact_iter(items.into_iter())
        .expect("the interpreter's allocator holds a host list")
}

// ---------------------------------------------------------------------------
// The embedder harness
// ---------------------------------------------------------------------------

/// Bytes one sixteen-bit IR word occupies.
const BYTES_PER_WORD: usize = 2;

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
/// A module the harness cannot start at all is outside that guarantee rather
/// than an eighth code inside it: this compiler declares no start function, so
/// a foreign module whose start traps is a harness surprise and not a verdict
/// about an artifact, and it panics — see [`run_with`]'s own `# Panics`.
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
    /// The invocation trapped.
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
/// interpreter harness above and reach none of this, and in a binary crate an
/// item nothing reaches is dead however public it is. Seeding liveness here
/// covers everything below, which is reachable from this function and from
/// nowhere else.
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
/// Panics for the reasons [`decode_with`] and [`LoadedModule::invoke`] do: an
/// interpreter that cannot be built at all, or a start function that does not
/// complete. Neither is a verdict about the artifact.
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
fn execute(command: &Command, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut session = SpaceWasmSession::acquire();
    let calls = HostCalls::default();
    let hosts = match host_stubs(&command.hosts, &calls) {
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

    let decoded = match hosts {
        None => decode(&mut session, &wasm),
        Some(hosts) => decode_with::<EMBEDDER_MAX_CONTROL_FRAMES, EMBEDDER_MAX_STACK_DEPTH>(
            &mut session,
            &wasm,
            hosts,
        ),
    };
    let mut module = match decoded {
        Ok(module) => module,
        Err(e) => {
            report_decode_failure(&command.module, &wasm, &e, &command.hosts, err);
            return exit::DECODE;
        }
    };
    // A start function runs inside the decode, and the calls it made are
    // printed before anything the command line asked for.
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
                host_export_refusal(&invocation.export, &export.import)
            ),
            None => format!("error: {path} exports no function `{}`", invocation.export),
        };
        line(err, &message);
        report_exports(&functions, &reexported, err);
        return exit::NO_SUCH_EXPORT;
    };

    let args = match arguments(signature, &invocation.args) {
        Ok(args) => args,
        Err(message) => {
            line(err, &format!("error: {message}"));
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
/// `None` when no spec was given, so that [`execute`] decodes through
/// [`decode`] rather than through [`decode_with`] with an empty set: the two
/// are the same load, and that call is what keeps `decode` live in the two
/// binaries that run a command line without the sweeps.
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
    specs: &[HostSpec],
    calls: &HostCalls,
) -> Result<Option<spacewasm::Vec<HostModule>>, String> {
    if specs.is_empty() {
        return Ok(None);
    }
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
            host_module(&first.module, functions, Vec::new()).map_err(|error| {
                name_refusal(first, "module", &first.module, HOST_MODULE_NAME_CAP, error)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(host_set(modules)))
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
/// interpreter's allocator, which is why it runs only while [`execute`] holds
/// the session.
fn report_decode_failure(
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
    let imports = imports_of(wasm);
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
    let refused_as_a_set = set_refusal(&imports);
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
fn set_refusal(imports: &[DeclaredImport]) -> Option<String> {
    let mut named = FxHashSet::default();
    let every_stub: Vec<HostSpec> = imports
        .iter()
        .filter_map(|import| import.stub.as_ref().ok())
        .filter(|spec| named.insert((spec.module.as_str(), spec.field.as_str())))
        .cloned()
        .collect();
    host_stubs(&every_stub, &HostCalls::default()).err()
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
fn imports_of(wasm: &[u8]) -> Vec<DeclaredImport> {
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
                        stub: stub_for(&import, &signatures),
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
    host_stubs(std::slice::from_ref(&spec), &HostCalls::default())
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

/// Coerces the arguments as written to what the export declared.
fn arguments(signature: &ExportedFunction, raw: &[String]) -> Result<Vec<Value>, String> {
    if raw.len() != signature.params.len() {
        return Err(format!(
            "`{}` takes {}; {} given",
            signature.name,
            arguments_phrase(signature.params.len()),
            arguments_phrase(raw.len())
        ));
    }
    raw.iter()
        .zip(signature.params.iter())
        .enumerate()
        .map(|(index, (text, ty))| match ty {
            ValType::I32 => text.parse::<i32>().map(Value::I32).map_err(|_| {
                format!(
                    "argument {} of `{}` is `i32`, and `{text}` is not one",
                    index + 1,
                    signature.name
                )
            }),
            ValType::I64 => text.parse::<i64>().map(Value::I64).map_err(|_| {
                format!(
                    "argument {} of `{}` is `i64`, and `{text}` is not one",
                    index + 1,
                    signature.name
                )
            }),
            ValType::F32 | ValType::F64 => Err(format!(
                "argument {} of `{}` is `{}`, and this harness passes decimal integers only: how \
                 a floating-point argument should be spelled on a command line is a question it \
                 does not have to settle",
                index + 1,
                signature.name,
                type_name(*ty)
            )),
        })
        .collect()
}

/// "1 argument" or "3 arguments".
fn arguments_phrase(count: usize) -> String {
    if count == 1 {
        "1 argument".to_string()
    } else {
        format!("{count} arguments")
    }
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

/// The WebAssembly spelling of a value type.
fn type_name(ty: ValType) -> &'static str {
    match ty {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
    }
}

/// A returned value as the command line prints it.
fn render(value: Value) -> String {
    match value {
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
    }
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
