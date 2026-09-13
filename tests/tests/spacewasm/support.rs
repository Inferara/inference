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
//! mut` cannot reach this crate's library or its `rocq-discharge` binaries. A
//! later example including this file by `#[path]` invokes it at its own root.

use std::alloc::Layout;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::{Mutex, MutexGuard, PoisonError};

use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, Engine, ExportDesc, HostModule, InnerVec,
    Interpreter, InterpreterResult, InterpreterRunner, InvokeError, Module, ModuleRef, ParseError,
    Ref, TrapReason, Value, WasmMemoryAllocator, WasmRef, WasmStream,
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
    /// How many values the *WebAssembly* signature takes. A hidden pointer the
    /// lowering introduced for an aggregate return is one of them, which is why
    /// this is read here and not off the source-level export descriptor.
    pub params: usize,
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
    _session: &'s mut SpaceWasmSession,
    wasm: &[u8],
    hosts: spacewasm::Vec<HostModule>,
) -> Result<LoadedModule<'s>, ParseError> {
    let mut code_builder = CodeBuilder::new(CompilerOptions {
        allow_memory_grow: false,
        max_backpatch_iterations: None,
        max_code_pages: MAX_CODE_PAGES,
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
                    params: owner.types[function.ty.0 as usize].params.len(),
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
