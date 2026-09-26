//! Loading a module into an engine of its own, starting it, and calling what
//! it exports.
//!
//! The two steps are two types, as WebAssembly has them. A load decodes,
//! validates and compiles the module and allocates what its instance holds —
//! its linear memory, its globals, its data segments — and runs nothing: a
//! [`LoadedModule`] can only be read. [`LoadedModule::start`] runs the start
//! function, and instantiation completes when it returns, as an [`Instance`],
//! the only type that calls an export. So a caller can resolve an export and
//! read its arguments against it, and refuse the call, before any code of the
//! module has executed — including a host call its start function makes.

use std::marker::PhantomData;
use std::num::NonZeroUsize;

use inference_target_conformance::spacewasm::{
    REFERENCE_MAX_CONTROL_FRAMES, REFERENCE_MAX_STACK_DEPTH,
};
use spacewasm::{
    CodeBuilder, CompilerOptions, Engine, ExportDesc, HostModuleRef, InnerVec, Interpreter,
    InterpreterResult, InterpreterRunner, MemoryError, Module, ModuleRef, Ref, TrapReason, ValType,
    Value, WasmRef, WasmStream,
};

use crate::errors::{ExportKind, InvokeError, LoadError, StartError, arity_clause};
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

/// Words of value stack the reference embedder's engine holds:
/// `spacewasm_std` builds its engine with `Engine::new(1024, …)`.
///
/// A call whose frames need more ends as a `StackOverflow` trap, which is the
/// reference configuration's verdict on that call chain rather than a property
/// of the interpreter.
pub const REFERENCE_STACK_WORDS: usize = 1024;

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

impl EngineConfig {
    /// The reference embedder's configuration, `spacewasm_std`'s:
    /// [`REFERENCE_STACK_WORDS`] of value stack and
    /// [`REFERENCE_MAX_CODE_PAGES`] of IR.
    pub const REFERENCE: Self =
        Self { stack_words: REFERENCE_STACK_WORDS, max_code_pages: REFERENCE_MAX_CODE_PAGES };
}

/// How many interpreter instructions a run may take.
///
/// The interpreter counts the instructions of its own compiled form of the
/// module, not WebAssembly's, so a budget depends on the interpreter release
/// and compares with no other engine's fuel.
///
/// One budget covers a whole run: the start function
/// [`LoadedModule::start`] runs, and then the call
/// [`Instance::invoke_within_budget`] or [`Instance::invoke_counting`] makes,
/// which is given what the start function left of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fuel {
    /// No budget: a run goes on until it returns or traps.
    Unbounded,
    /// At most this many instructions.
    Limited(NonZeroUsize),
}

/// A module decoded, validated and compiled to IR inside an engine of its own,
/// with its linear memory, its globals and its data segments in place, and
/// none of its code run.
///
/// Everything a `LoadedModule` offers reads it: what it exports, what one
/// export takes, what it compiled to. [`LoadedModule::start`] is the one way
/// to run any of it, and it gives the module up for the [`Instance`] that
/// calls its exports. A loaded module has no call of its own, with a budget
/// of its own or within one:
///
/// ```compile_fail,E0599
/// fn call(module: &mut inference_spacewasm_runner::LoadedModule<'_>) {
///     let _ = module.invoke("main", &[], 1_000);
/// }
/// ```
///
/// ```compile_fail,E0599
/// fn call(module: &mut inference_spacewasm_runner::LoadedModule<'_>) {
///     let _ = module.invoke_within_budget("main", &[]);
/// }
/// ```
///
/// It is not `Send`, since it borrows a [`Session`], which is not either:
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<inference_spacewasm_runner::LoadedModule<'static>>();
/// ```
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

/// A loaded module whose start function has returned, or that declares none:
/// the only thing that calls an export.
///
/// [`Instance::module`] reads it as the [`LoadedModule`] it was started from,
/// which cannot be started through it again. It borrows the session its
/// module was loaded under, as the module did, and so is not `Send` either:
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<inference_spacewasm_runner::Instance<'static>>();
/// ```
pub struct Instance<'session> {
    module: LoadedModule<'session>,
    /// The budget the module was started under.
    budget: Fuel,
    /// Instructions its start function spent of [`Self::budget`]: exact under
    /// a limited budget, and zero when there is no start function or no limit.
    spent_by_start: usize,
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

impl ExportedFunction {
    /// What the function takes, as the clause a sentence about its arguments
    /// opens with: "`main` takes no arguments", "`echo` takes 1 argument
    /// (i64)" or "`add` takes 2 arguments (i32, i32)".
    #[must_use]
    pub fn arity_clause(&self) -> String {
        arity_clause(&self.name, &self.params)
    }
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
    OutOfFuel {
        /// The budget that ran out: every instruction of it was spent. For
        /// [`Instance::invoke_within_budget`] and [`Instance::invoke_counting`]
        /// that is the whole budget the module was started under, start
        /// function included.
        budget: usize,
    },
}

/// How a call ended, and how many of the interpreter's instructions it took:
/// what [`Instance::invoke_counting`] answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Counted {
    /// How the call ended.
    pub outcome: Outcome,
    /// Interpreter instructions the call executed, its closing return or the
    /// instruction that trapped included, so a budget of exactly this many
    /// ends the call the same way and one fewer runs out. A call that ran out
    /// of fuel executed every instruction it was given; a call the engine
    /// refused to begin for want of stack executed none.
    pub instructions: usize,
}

/// How one run of the engine ended.
///
/// [`InterpreterResult`] with the budget attached to the one ending that has
/// one, at the only place that knows it: an unbounded run resumes every run the
/// interpreter stops for fuel, so it never ends out of fuel.
enum Ended {
    Finished,
    Trapped(TrapReason),
    Paused,
    OutOfFuel { budget: usize },
}

/// What a call may spend.
#[derive(Clone, Copy)]
enum Allowance {
    /// At most `left` instructions, reported as `budget` when they run out.
    Limited { left: usize, budget: usize },
    /// As many as the call takes.
    Unbounded,
}

/// Loads `wasm` at the reference embedder's verifier bounds, with no host
/// module registered, and runs nothing.
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
    config: EngineConfig,
) -> Result<LoadedModule<'s>, LoadError> {
    load_with::<{ REFERENCE_MAX_CONTROL_FRAMES as usize }, { REFERENCE_MAX_STACK_DEPTH as usize }>(
        session,
        wasm,
        spacewasm::Vec::zero(),
        config,
    )
}

/// Loads `wasm` with the verifier's two stack bounds and the host set chosen by
/// the caller, and runs nothing.
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
/// code builder is told to refuse `memory.grow`, as `spacewasm_std` tells its
/// own; the target's conformance check leaves that choice to the embedder, so
/// a module it accepts can still be refused here for one.
///
/// A module that loads has its memory, its globals and its data segments in
/// place, and its start function not yet run: nothing of it runs until
/// [`LoadedModule::start`], so no host in `hosts` is called by a load.
///
/// # Errors
///
/// [`LoadError::Decode`] when the interpreter refuses the bytes, and
/// [`LoadError::Resource`] when it cannot be built to try.
pub fn load_with<'s, const CONTROL_FRAMES: usize, const STACK_DEPTH: usize>(
    _session: &'s mut Session,
    wasm: &[u8],
    hosts: HostSet,
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

    Ok(LoadedModule { engine, code_builder, module, session: PhantomData })
}

/// The refusal for an interpreter that could not allocate `part`.
fn resource(part: &'static str) -> impl FnOnce(MemoryError) -> LoadError {
    move |error| LoadError::Resource { part, error }
}

impl<'session> LoadedModule<'session> {
    /// Runs the module's start function, if it declares one, under `fuel`,
    /// and gives the module up for the [`Instance`] that calls its exports.
    ///
    /// `fuel` is the budget of the whole run: the start function's, and then
    /// the call [`Instance::invoke_within_budget`] or
    /// [`Instance::invoke_counting`] makes. Under a limit the
    /// start function is stepped one instruction at a time, because the
    /// interpreter says how a run ended and never how much of its budget it
    /// spent, and stepping is the one way to leave that call exactly the rest.
    /// Stepping costs a start function a call into the interpreter per
    /// instruction; this compiler never emits one. A module without a start
    /// function spends nothing here, and leaves the call the whole budget.
    ///
    /// A start function that does not return leaves no instance, as a
    /// WebAssembly instantiation whose start function traps leaves none: the
    /// module is dropped with its engine.
    ///
    /// # Errors
    ///
    /// A [`StartError`] when the start function does not return: it traps —
    /// its frame not fitting the value stack among the ways, as
    /// `StackOverflow` — it runs out of `fuel`, a host function it calls
    /// pauses it, or the engine refuses to begin it.
    pub fn start(mut self, fuel: Fuel) -> Result<Instance<'session>, StartError> {
        let spent_by_start = self.run_start(fuel)?;
        Ok(Instance { module: self, budget: fuel, spent_by_start })
    }

    /// Runs the module's start function, if it declares one, under `fuel`,
    /// and answers the instructions it spent: exactly under a limit, zero
    /// without one.
    fn run_start(&mut self, fuel: Fuel) -> Result<usize, StartError> {
        let Some(start) = self.engine.module_start(self.module) else {
            return Ok(0);
        };
        match self.engine.invoke(start, &[]) {
            Ok(()) => {}
            Err(spacewasm::InvokeError::StackOverflow) => {
                return Err(StartError::Trapped(TrapReason::StackOverflow));
            }
            Err(refusal) => {
                return Err(StartError::Refused { refusal: format!("{refusal:?}") });
            }
        }
        let (ended, spent) = match fuel {
            Fuel::Unbounded => (self.run_unbounded(), 0),
            Fuel::Limited(budget) => {
                self.run_stepped(Allowance::Limited { left: budget.get(), budget: budget.get() })
            }
        };
        match ended {
            Ended::Finished => Ok(spent),
            Ended::Trapped(reason) => Err(StartError::Trapped(reason)),
            Ended::OutOfFuel { budget } => Err(StartError::OutOfFuel { budget }),
            Ended::Paused => Err(StartError::Paused),
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
    /// loaded does, and [`Instance::invoke`] refuses such a name as
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

    /// The function this module exports under `name`, with its WebAssembly
    /// signature, or the refusal [`Instance::invoke`] would give a call of it.
    ///
    /// It resolves the name as a call does and stops there, so a caller can
    /// read what a function takes before it decides to start the module and
    /// call it — and, taking the module shared, it cannot run any of it.
    ///
    /// # Errors
    ///
    /// [`InvokeError::NoSuchExport`] when the module exports nothing under
    /// `name`, [`InvokeError::NotAFunction`] when it exports a memory, a table
    /// or a global, and [`InvokeError::HostReexport`] when it exports a host
    /// import again.
    pub fn function(&self, name: &str) -> Result<ExportedFunction, InvokeError> {
        let reference = self.resolve(name)?;
        let (params, result) = self.signature(reference);
        Ok(ExportedFunction { name: name.to_string(), params, result })
    }

    /// Calls `export` with `args`, running the engine with `run` once the call
    /// has begun.
    ///
    /// Only an [`Instance`] reaches this, which is what keeps a loaded module
    /// from running anything before [`LoadedModule::start`] has run its start
    /// function. `run` is not called for a call the engine refuses to begin.
    fn call(
        &mut self,
        export: &str,
        args: &[Value],
        run: impl FnOnce(&mut Self) -> Ended,
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
        match run(self) {
            Ended::Finished => match (result_ty, self.engine.result) {
                (None, _) => Ok(Outcome::Returned(None)),
                (Some(ty), Some(raw)) => Ok(Outcome::Returned(Some(raw.to_value(ty)))),
                (Some(_), None) => Err(InvokeError::NoResult { export: export.to_string() }),
            },
            Ended::Trapped(reason) => Ok(Outcome::Trapped(reason)),
            Ended::OutOfFuel { budget } => {
                self.engine.reset();
                Ok(Outcome::OutOfFuel { budget })
            }
            Ended::Paused => {
                self.engine.reset();
                Err(InvokeError::Paused { export: export.to_string() })
            }
        }
    }

    /// Runs the engine from where an `invoke` left it, spending at most what
    /// `allowance` allows, without counting what it spends.
    fn run_uncounted(&mut self, allowance: Allowance) -> Ended {
        match allowance {
            Allowance::Limited { left, budget } => self.run_limited(left, budget),
            Allowance::Unbounded => self.run_unbounded(),
        }
    }

    /// Runs the engine from where an `invoke` left it for at most `left`
    /// instructions, reporting `budget` if they run out.
    fn run_limited(&mut self, left: usize, budget: usize) -> Ended {
        match self.run(left) {
            InterpreterResult::Finished => Ended::Finished,
            InterpreterResult::Trap(reason) => Ended::Trapped(reason),
            InterpreterResult::Pause => Ended::Paused,
            InterpreterResult::OutOfFuel => Ended::OutOfFuel { budget },
        }
    }

    /// Runs the engine from where an `invoke` left it until the call returns,
    /// traps or pauses, resuming it each time the interpreter stops it for
    /// fuel.
    fn run_unbounded(&mut self) -> Ended {
        loop {
            match self.run(usize::MAX) {
                InterpreterResult::Finished => return Ended::Finished,
                InterpreterResult::Trap(reason) => return Ended::Trapped(reason),
                InterpreterResult::Pause => return Ended::Paused,
                InterpreterResult::OutOfFuel => {}
            }
        }
    }

    /// Runs the engine from where an `invoke` left it one instruction at a
    /// time, spending at most what `allowance` allows, and answers how the run
    /// ended and how many instructions it took.
    ///
    /// Every step executes exactly one instruction, the last included: the
    /// interpreter reports a call's return, and a trap, from inside the step
    /// that executes it, which is also why a single run given exactly as many
    /// instructions as a call takes ends it the same way. Without a limit the
    /// count saturates at `usize::MAX` rather than wrapping, which no run
    /// reaches.
    fn run_stepped(&mut self, allowance: Allowance) -> (Ended, usize) {
        let mut spent: usize = 0;
        loop {
            if let Allowance::Limited { left, budget } = allowance
                && spent == left
            {
                return (Ended::OutOfFuel { budget }, spent);
            }
            spent = spent.saturating_add(1);
            let ended = match self.run(1) {
                InterpreterResult::Finished => Ended::Finished,
                InterpreterResult::Trap(reason) => Ended::Trapped(reason),
                InterpreterResult::Pause => Ended::Paused,
                InterpreterResult::OutOfFuel => continue,
            };
            return (ended, spent);
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
        let not_a_function = |kind| InvokeError::NotAFunction { export: export.to_string(), kind };
        let index = match entry.desc {
            ExportDesc::Func(index) => index,
            ExportDesc::Mem(_) => return Err(not_a_function(ExportKind::Memory)),
            ExportDesc::Table(_) => return Err(not_a_function(ExportKind::Table)),
            ExportDesc::Global(_) => return Err(not_a_function(ExportKind::Global)),
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

impl<'session> Instance<'session> {
    /// The module this instance was started from, for what it exports and
    /// what it compiled to.
    #[must_use]
    pub fn module(&self) -> &LoadedModule<'session> {
        &self.module
    }

    /// Calls `export` with `args` under a `fuel`-instruction budget of its
    /// own, whatever the budget the module was started under.
    ///
    /// The arguments are values, already of the types the function declares;
    /// [`crate::coerce_arguments`] reads them from text. Whatever the call
    /// does, the engine is idle again when this returns, so the instance can be
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
        let allowance = Allowance::Limited { left: fuel, budget: fuel };
        self.module.call(export, args, |module| module.run_uncounted(allowance))
    }

    /// Calls `export` with `args` under what the start function left of the
    /// budget the module was started under.
    ///
    /// Under [`Fuel::Unbounded`] the call runs until it returns or traps. Under
    /// [`Fuel::Limited`] it may spend the budget less what the start function
    /// spent, and an [`Outcome::OutOfFuel`] reports the whole budget, since
    /// that is how many instructions the run took. A call that returns does
    /// not say how many it spent, so every call made this way is given the same
    /// remainder: the budget bounds a run of the start function and one call.
    /// [`Instance::invoke_counting`] makes the same call and counts what it
    /// spends, at a cost.
    ///
    /// # Errors
    ///
    /// As [`Instance::invoke`].
    pub fn invoke_within_budget(
        &mut self,
        export: &str,
        args: &[Value],
    ) -> Result<Outcome, InvokeError> {
        let allowance = self.remainder();
        self.module.call(export, args, |module| module.run_uncounted(allowance))
    }

    /// Calls `export` with `args` as [`Instance::invoke_within_budget`] does,
    /// under what the start function left of the budget, and counts the
    /// interpreter instructions the call takes.
    ///
    /// The interpreter says how a run ended and never how much of its budget
    /// it spent, so the call is run one instruction at a time, as a start
    /// function under a limit is. That costs one re-entry into the interpreter
    /// per instruction the call executes, which is what makes the count exact
    /// and a counted call slower than an uncounted one: this is a measurement,
    /// not the way to run a program.
    ///
    /// The count is the call's alone, never the start function's, and it is
    /// exact: a call that began and then returned or trapped ends the same way
    /// when [`Instance::invoke`] makes it with a budget of exactly
    /// [`Counted::instructions`], and runs out of fuel with one fewer. It is
    /// reported and never deducted, so a later call is given the same
    /// remainder, as every call made within the budget is. Under
    /// [`Fuel::Unbounded`] the count saturates at `usize::MAX` rather than
    /// wrapping, which no run reaches.
    ///
    /// # Errors
    ///
    /// As [`Instance::invoke`].
    pub fn invoke_counting(
        &mut self,
        export: &str,
        args: &[Value],
    ) -> Result<Counted, InvokeError> {
        let allowance = self.remainder();
        let mut instructions = 0;
        let outcome = self.module.call(export, args, |module| {
            let (ended, spent) = module.run_stepped(allowance);
            instructions = spent;
            ended
        })?;
        Ok(Counted { outcome, instructions })
    }

    /// What the start function left of the budget the module was started
    /// under.
    fn remainder(&self) -> Allowance {
        match self.budget {
            Fuel::Unbounded => Allowance::Unbounded,
            Fuel::Limited(budget) => Allowance::Limited {
                left: budget.get().saturating_sub(self.spent_by_start),
                budget: budget.get(),
            },
        }
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
