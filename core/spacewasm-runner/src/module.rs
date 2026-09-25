//! Loading a module into an engine of its own, and calling what it exports.

use std::marker::PhantomData;

use inference_target_conformance::spacewasm::{
    REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH,
};
use spacewasm::{
    CodeBuilder, CompilerOptions, Engine, ExportDesc, HostModuleRef, InnerVec, Interpreter,
    InterpreterResult, InterpreterRunner, MemoryError, Module, ModuleRef, Ref, TrapReason, ValType,
    Value, WasmRef, WasmStream,
};

use crate::errors::{InvokeError, LoadError};
use crate::hosts::HostSet;
use crate::session::{Session, StdAllocator};

/// IR pages the code builder may fill in the reference embedder's
/// configuration, `spacewasm_std`'s.
///
/// A page holds 256 sixteen-bit words, so this is 128 KiB of compiled IR. A
/// module needing more fails to load rather than silently growing the budget,
/// which is the honest reading for a target whose premise is a fixed memory
/// envelope.
pub const REFERENCE_MAX_CODE_PAGES: usize = 256;

/// Modules one engine holds. Each load builds its own engine, so one is all a
/// load ever needs.
const MAX_MODULES: usize = 1;

/// Sixteen-bit words one IR page holds.
const WORDS_PER_PAGE: usize = 256;

/// Bytes one sixteen-bit IR word occupies.
const BYTES_PER_WORD: usize = 2;

/// The two parts of an engine's configuration an embedder chooses at run time.
///
/// The verifier's two stack bounds are chosen at compile time instead, as the
/// const generics of [`load_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    /// Words the value stack holds. A call whose frames need more ends as a
    /// `StackOverflow` trap.
    pub stack_words: usize,
    /// IR pages the code builder may fill. A module whose compiled form needs
    /// more is refused as it loads; [`REFERENCE_MAX_CODE_PAGES`] is the
    /// reference embedder's budget.
    pub max_code_pages: usize,
}

/// A module decoded, compiled to IR and instantiated inside an engine of its
/// own, with its start function run.
///
/// The `'session` lifetime is the safety argument: the engine, its IR pages
/// and the linear memory are all freed through the interpreter's process-wide
/// allocator, so this value may not outlive the lock that serializes it.
pub struct LoadedModule<'session> {
    engine: Engine,
    code_builder: CodeBuilder,
    module: ModuleRef,
    session: PhantomData<&'session mut Session>,
}

/// One exported function, as the decoder sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedFunction {
    /// The name in the export section.
    pub name: String,
    /// What the *WebAssembly* signature takes, in order. A hidden pointer the
    /// lowering introduced for an aggregate return is one of them, which is
    /// why this is read here and not off the source-level export descriptor —
    /// and the types rather than a count, because a caller supplying arguments
    /// on a command line has to know which of them the engine wants 64 bits
    /// wide.
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

/// How a call ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It returned, with its result if it declared one.
    Returned(Option<Value>),
    /// It trapped. A call the engine refuses to begin because its frames do
    /// not fit the value stack is one of these, with `StackOverflow`, as wasmtime
    /// reports its own exhausted stack.
    Trapped(TrapReason),
    /// It was still running when the instruction budget ran out. The call is
    /// abandoned and the engine is idle again.
    OutOfFuel,
}

/// Loads `wasm` at the reference embedder's verifier bounds, with no host
/// module registered, and runs its start function under `start_fuel`
/// instructions.
///
/// The bounds are [`REFERENCE_MAX_CONTROL_FRAMES`] and
/// [`REFERENCE_MAX_STACK_DEPTH`]; [`load_with`] takes others.
///
/// # Errors
///
/// As [`load_with`].
pub fn load<'s>(
    session: &'s mut Session,
    wasm: &[u8],
    start_fuel: usize,
    config: EngineConfig,
) -> Result<LoadedModule<'s>, LoadError> {
    load_with::<{ REFERENCE_MAX_CONTROL_FRAMES as usize }, { REFERENCE_MAX_STACK_DEPTH as usize }>(
        session,
        wasm,
        spacewasm::Vec::zero(),
        start_fuel,
        config,
    )
}

/// Loads `wasm` with the verifier's two stack bounds and the host set chosen by
/// the caller, and runs its start function under `start_fuel` instructions.
///
/// The bounds are const generics in the decoder because they size stacks the
/// verifier walks with, so a tighter embedder is expressed by loading again
/// rather than by reading a number off a report. The host set is built with
/// [`crate::host_module`] and [`crate::host_set`]; the interpreter binds every
/// import to it while it decodes, so an import it does not supply is a decode
/// refusal.
///
/// The module is loaded under the empty name. A named module shares the
/// namespace host modules are registered in, and the interpreter refuses one
/// whose name a host module already carries as `DuplicateModuleName` before it
/// reads a section — a guest named `env` beside the host module `env`. The
/// empty name is exempt from that check, and nothing needs the guest's name:
/// its engine holds this one module, so no other module imports from it. The
/// code builder refuses `memory.grow`, which the target does not allow.
///
/// # Errors
///
/// [`LoadError::Decode`] when the interpreter refuses the bytes,
/// [`LoadError::Resource`] when it cannot be built to try, and the `Start*`
/// variants when the module's start function does not return.
pub fn load_with<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    _session: &'s mut Session,
    wasm: &[u8],
    hosts: HostSet,
    start_fuel: usize,
    config: EngineConfig,
) -> Result<LoadedModule<'s>, LoadError> {
    let mut code_builder = CodeBuilder::new(CompilerOptions {
        allow_memory_grow: false,
        max_backpatch_iterations: None,
        max_code_pages: config.max_code_pages,
    })
    .map_err(MemoryError::from)
    .map_err(resource("its IR code pages"))?;
    let mut engine = Engine::new(config.stack_words, MAX_MODULES, hosts)
        .map_err(resource("its engine"))?;
    let memory_allocator = spacewasm::Rc::new(StdAllocator)
        .map_err(MemoryError::from)
        .map_err(resource("a linear-memory allocator"))?
        .into_wasm_memory_allocator();

    let mut stream = ByteStream::new(wasm);
    let module = Module::new::<CONTROL_FRAMES, STACK_DEPTH>(
        "",
        &mut stream,
        &mut engine.store,
        &mut code_builder,
        memory_allocator,
    )
    .map_err(LoadError::Decode)?;
    let module = engine
        .push_module(module)
        .map_err(MemoryError::from)
        .map_err(resource("a place for the module in its store"))?;

    let mut loaded = LoadedModule { engine, code_builder, module, session: PhantomData };
    loaded.run_start(start_fuel)?;
    Ok(loaded)
}

/// The refusal for an interpreter that could not allocate `part`.
fn resource(part: &'static str) -> impl FnOnce(MemoryError) -> LoadError {
    move |error| LoadError::Resource { part, error }
}

impl LoadedModule<'_> {
    /// Runs the module's start function, if it declares one, under `fuel`
    /// instructions.
    fn run_start(&mut self, fuel: usize) -> Result<(), LoadError> {
        let Some(start) = self.engine.module_start(self.module) else {
            return Ok(());
        };
        match self.engine.invoke(start, &[]) {
            Ok(()) => {}
            Err(spacewasm::InvokeError::StackOverflow) => {
                return Err(LoadError::StartTrapped(TrapReason::StackOverflow));
            }
            Err(refusal) => {
                return Err(LoadError::StartRefused { refusal: format!("{refusal:?}") });
            }
        }
        match self.run(fuel) {
            InterpreterResult::Finished => Ok(()),
            InterpreterResult::Trap(reason) => Err(LoadError::StartTrapped(reason)),
            InterpreterResult::OutOfFuel => Err(LoadError::StartOutOfFuel { budget: fuel }),
            InterpreterResult::Pause => Err(LoadError::StartPaused),
        }
    }

    /// Every function this module exports, in export-section order.
    ///
    /// A host import the module exports again is left out, because the engine
    /// invokes only WebAssembly functions and a host function has no body here
    /// to run; [`LoadedModule::exported_host_imports`] lists those.
    ///
    /// An export whose function index does not resolve is skipped too, without
    /// a word. The decoder refuses a module holding one, so no module that
    /// loaded does, and [`LoadedModule::invoke`] refuses such a name as
    /// [`InvokeError::NoSuchExport`] should one ever appear.
    #[must_use]
    pub fn exported_functions(&self) -> Vec<ExportedFunction> {
        let module = self.guest();
        module
            .exports
            .iter()
            .filter_map(|export| {
                let ExportDesc::Func(index) = export.desc else {
                    return None;
                };
                let reference = match module.get_func_ref(index)? {
                    Ref::Module(local) => WasmRef { module: self.module, index: local },
                    Ref::Extern { module, index } => WasmRef { module, index },
                    Ref::Host { .. } => return None,
                };
                let (params, result) = self.signature(reference);
                Some(ExportedFunction { name: export.name.to_string(), params, result })
            })
            .collect()
    }

    /// Every host import this module exports again, in export-section order:
    /// the exports [`LoadedModule::exported_functions`] leaves out.
    ///
    /// This is how a caller tells a reader what those names are, both where it
    /// lists the exports and where one is asked for by name — without it, the
    /// export section would read as shorter than it is.
    #[must_use]
    pub fn exported_host_imports(&self) -> Vec<ReexportedImport> {
        let module = self.guest();
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

    /// Calls `export` with `args` under a `fuel`-instruction budget.
    ///
    /// The arguments are values, already of the types the function declares;
    /// [`crate::coerce_arguments`] reads them from text. Whatever the call
    /// does, the engine is idle again when this returns, so the module can be
    /// called once more.
    ///
    /// # Errors
    ///
    /// An [`InvokeError`] when the call cannot be made: `export` names no
    /// function this module exports, or names a host import it exports again,
    /// or `args` does not match the function's parameters — or when a host
    /// function the call reaches pauses it, which only an embedder that
    /// resumes the call could honour, or when the call finishes without the
    /// result the function declares, which only an interpreter defect does.
    pub fn invoke(
        &mut self,
        export: &str,
        args: &[Value],
        fuel: usize,
    ) -> Result<Outcome, InvokeError> {
        let reference = self.resolve(export)?;
        let (params, result_ty) = self.signature(reference);
        match self.engine.invoke(reference, args) {
            Ok(()) => {}
            Err(spacewasm::InvokeError::StackOverflow) => {
                return Ok(Outcome::Trapped(TrapReason::StackOverflow));
            }
            Err(spacewasm::InvokeError::ParamLenMismatch) => {
                return Err(InvokeError::Arity {
                    export: export.to_string(),
                    expected: params.len(),
                    given: args.len(),
                });
            }
            Err(spacewasm::InvokeError::ParamTypeMismatch) => {
                return Err(InvokeError::Argument {
                    export: export.to_string(),
                    expected: params,
                    given: args.iter().map(|value| value_type(*value)).collect(),
                });
            }
            Err(spacewasm::InvokeError::Busy) => {
                return Err(InvokeError::Busy { export: export.to_string() });
            }
        }
        match self.run(fuel) {
            InterpreterResult::Finished => match (result_ty, self.engine.result) {
                (None, _) => Ok(Outcome::Returned(None)),
                (Some(ty), Some(raw)) => Ok(Outcome::Returned(Some(raw.to_value(ty)))),
                (Some(_), None) => Err(InvokeError::NoResult { export: export.to_string() }),
            },
            InterpreterResult::Trap(reason) => Ok(Outcome::Trapped(reason)),
            InterpreterResult::OutOfFuel => {
                self.engine.reset();
                Ok(Outcome::OutOfFuel)
            }
            InterpreterResult::Pause => {
                self.engine.reset();
                Err(InvokeError::Paused { export: export.to_string() })
            }
        }
    }

    /// Runs the engine from where an `invoke` left it, under `fuel`
    /// instructions.
    fn run(&mut self, fuel: usize) -> InterpreterResult {
        Interpreter.run(self.code_builder.pages(), &mut self.engine, fuel)
    }

    /// The module this engine was built for, as its store holds it.
    fn guest(&self) -> &Module {
        &self.engine.store.modules()[usize::from(self.module.0)]
    }

    /// The parameter types and the result type of the function `reference`
    /// names.
    fn signature(&self, reference: WasmRef) -> (Vec<ValType>, Option<ValType>) {
        let owner = &self.engine.store.modules()[usize::from(reference.module.0)];
        let function = &owner.functions[usize::from(reference.index)];
        let params = owner.types[function.ty.0 as usize].params.iter().copied().collect();
        (params, function.return_ty)
    }

    /// Resolves an export name to the reference the engine invokes through.
    fn resolve(&self, export: &str) -> Result<WasmRef, InvokeError> {
        let module = self.guest();
        let no_such_export = || InvokeError::NoSuchExport {
            export: export.to_string(),
            exports: self.exported_functions().into_iter().map(|function| function.name).collect(),
        };
        let entry = module
            .exports
            .iter()
            .find(|entry| entry.name == export)
            .ok_or_else(no_such_export)?;
        let ExportDesc::Func(index) = entry.desc else {
            return Err(InvokeError::NotAFunction { export: export.to_string() });
        };
        match module.get_func_ref(index).ok_or_else(no_such_export)? {
            Ref::Module(local) => Ok(WasmRef { module: self.module, index: local }),
            Ref::Extern { module, index } => Ok(WasmRef { module, index }),
            Ref::Host { module, index } => Err(InvokeError::HostReexport {
                export: export.to_string(),
                import: self.host_function_name(module, index),
            }),
        }
    }

    /// The `module.field` a host function was registered under.
    fn host_function_name(&self, module: HostModuleRef, index: u16) -> String {
        let host = &self.engine.store.host_modules()[usize::from(module.0)];
        format!("{}.{}", host.name.as_str(), host.functions[usize::from(index)].name())
    }
}

/// The type of `value`.
fn value_type(value: Value) -> ValType {
    match value {
        Value::I32(_) => ValType::I32,
        Value::I64(_) => ValType::I64,
        Value::F32(_) => ValType::F32,
        Value::F64(_) => ValType::F64,
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
    /// The divisor is non-zero when the `wasm_len` given to [`ir_stats`] is the
    /// length of the bytes the module was loaded from, which is the caller's to
    /// keep: a module that loaded carried at least a magic number and a
    /// version. Given zero instead, the ratio is infinite or not a number
    /// rather than a panic. Both counts are far below the 2^53 an `f64` holds
    /// exactly.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn ir_bytes_per_wasm_byte(self) -> f64 {
        self.ir_bytes() as f64 / self.wasm_bytes as f64
    }
}

/// Measures the IR `module` compiled to, against the `wasm_len` bytes it came
/// from.
///
/// Both figures are computed as upstream's `spacewasm_std` computes its own —
/// pages held, and every page but the last counted full plus the writer's
/// offset into the last — so they read line for line beside its output.
#[must_use]
pub fn ir_stats(module: &LoadedModule<'_>, wasm_len: usize) -> IrStats {
    let pages = module.code_builder.pages().len();
    let filled = pages.saturating_sub(1) * WORDS_PER_PAGE + module.code_builder.offset();
    IrStats { code_pages: pages, ir_words: filled, wasm_bytes: wasm_len }
}

/// A [`WasmStream`] over one in-memory module.
///
/// The decoder pulls its input in chunks and hands each one back through
/// [`WasmStream::return_`] so a flight embedder can reuse a fixed buffer. This
/// one owns a single chunk holding the whole module and lends it out once: the
/// buffer is never transferred, so the hand-back is a no-op and nothing is
/// leaked or double-freed.
struct ByteStream {
    buffer: Vec<u8>,
    lent: bool,
}

impl ByteStream {
    /// Wraps a copy of `wasm`.
    fn new(wasm: &[u8]) -> Self {
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
        Ok(Some(unsafe { InnerVec::from_raw_parts(self.buffer.as_mut_ptr(), len, len) }))
    }

    fn return_(&mut self, _chunk: InnerVec<u8>) {
        // Nothing to reclaim, and nothing this body could reclaim: an
        // `InnerVec` owns no allocation and has no `Drop` of its own, so the
        // chunk falling out of scope frees nothing. The buffer it views belongs
        // to `self` and stays alive for the whole decode.
    }
}
