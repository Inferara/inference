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
//! contract, so the lock is not a convention a test is asked to remember: it is
//! a token no test can forge, and the only route to the interpreter is
//! [`SpaceWasmSession::acquire`] followed by [`decode`]. A [`LoadedModule`]
//! additionally *borrows* the session it was decoded under, so a module cannot
//! outlive the lock that protects the allocator it will be freed through — the
//! case a bare token would still let a test write.
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

use std::alloc::Layout;
use std::io::Write;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard, PoisonError};

use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, Engine, ExportDesc, HostModule, InnerVec,
    Interpreter, InterpreterResult, InterpreterRunner, InvokeError, Module, ModuleRef, ParseError,
    Ref, TrapReason, ValType, Value, WasmMemoryAllocator, WasmRef, WasmStream,
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
/// for the callers that register one — an embedder harness measuring a bounded
/// configuration, and host-import support (#464) — while every call site in
/// this tier passes none.
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
        "main",
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
    /// Panics if `export` is not an exported function of this module, or if the
    /// argument list does not match its signature — both harness faults, not
    /// program outcomes.
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
            InterpreterResult::Pause => {
                panic!("`{export}` paused, which no module without host imports can do")
            }
        }
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
            Ref::Host { .. } => panic!("`{export}` resolves to a host function"),
        }
    }
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
// The embedder harness
// ---------------------------------------------------------------------------

/// Bytes one sixteen-bit IR word occupies.
const BYTES_PER_WORD: usize = 2;

/// How to call the harness, printed under every argument-level refusal.
///
/// Spelled apart from [`exit::USAGE`], which is the code the same refusal exits
/// with: one file holding two `USAGE`s would leave a reader to tell a banner
/// from an exit code by its path.
const USAGE_LINE: &str =
    "usage: spacewasm-embed <module.wasm> [--invoke NAME [ARG…]] [--fuel N] [--stats [--json]]";

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
    /// The module loaded but exports no such function.
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
}

/// The export to call and the arguments to call it with, as written.
///
/// The arguments stay text until the module is loaded: what `-1` means depends
/// on the declared parameter type, which only the artifact knows.
struct Invocation {
    export: String,
    args: Vec<String>,
}

impl Command {
    /// Reads `argv`, which excludes the program name.
    ///
    /// A token starting with `--` ends `--invoke`'s argument list, which is what
    /// lets `--invoke f -1 --fuel 10` mean what it looks like: a negative
    /// argument is one hyphen and an option is two.
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
        };

        let value = |index: usize, option: &str, what: &str| -> Result<&str, String> {
            argv.get(index)
                .map(String::as_str)
                .filter(|token| !token.starts_with("--"))
                .ok_or_else(|| format!("`{option}` needs {what}"))
        };

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
fn execute(command: &Command, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let wasm = match std::fs::read(&command.module) {
        Ok(wasm) => wasm,
        Err(e) => {
            line(err, &format!("error: cannot read {}: {e}", command.module.display()));
            return exit::UNREADABLE;
        }
    };

    let mut session = SpaceWasmSession::acquire();
    let mut module = match decode(&mut session, &wasm) {
        Ok(module) => module,
        Err(e) => {
            report_decode_failure(&command.module, &wasm, &e, err);
            return exit::DECODE;
        }
    };

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
    let Some(invocation) = command.invoke.as_ref() else {
        if !command.stats {
            // A run that asked for nothing still says what it found, or a decode
            // check would be indistinguishable from a harness that did nothing.
            line(out, &format!("loaded {}", command.module.display()));
            report_exports(&functions, out);
        }
        return exit::OK;
    };

    let Some(signature) = functions.iter().find(|f| f.name == invocation.export) else {
        line(
            err,
            &format!(
                "error: {} exports no function `{}`",
                command.module.display(),
                invocation.export
            ),
        );
        report_exports(&functions, err);
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

    match module.invoke(&invocation.export, &args, command.fuel) {
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
fn report_exports(functions: &[ExportedFunction], sink: &mut dyn Write) {
    if functions.is_empty() {
        line(sink, "  it exports no functions");
        return;
    }
    line(sink, "  it exports:");
    for function in functions {
        line(sink, &format!("    {}", describe(function)));
    }
}

/// Reports bytes the interpreter refused, naming any import the module carries.
///
/// The decoder's own error says only that an import went unresolved; which one
/// is not in it, because a flight decoder keeps no string it does not have to.
/// So the module is read a second time for the one thing the verdict is
/// missing.
///
/// Every refused module that declares an import gets the list, not only the one
/// whose verdict was about an import. Deciding otherwise would mean naming the
/// variants of the decoder's own error that mean "nothing supplied this", which
/// is a guess at somebody else's taxonomy that goes quietly wrong when they add
/// one — and the sentence is true either way, since this harness registers no
/// host module and an import-bearing module was never going to load under it.
/// It is worded as a fact about the module rather than as a diagnosis, so a
/// reader is not told the import caused a failure the line above already
/// attributed.
fn report_decode_failure(path: &Path, wasm: &[u8], error: &ParseError, err: &mut dyn Write) {
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
    line(err, "  it imports these, and this harness registers no host module to supply them:");
    for import in &imports {
        line(err, &format!("    {import}"));
    }
    line(
        err,
        "  binding an `external fn` to something the embedder supplies at run time is tracked \
         as issue #464",
    );
}

/// Every import the module declares, as `module.field`.
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
fn imports_of(wasm: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in inf_wasmparser::Parser::new(0).parse_all(wasm).flatten() {
        let inf_wasmparser::Payload::ImportSection(section) = payload else {
            continue;
        };
        for import in section.into_iter().flatten() {
            names.push(format!("{}.{}", import.module, import.name));
        }
    }
    names
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
