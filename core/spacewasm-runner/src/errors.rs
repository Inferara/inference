//! Every way loading a module, starting it, calling one of its functions,
//! building a host set or reading an argument can fail.
//!
//! A trap and an exhausted instruction budget of a call are not here: they are
//! how a call ends, not a failure to make it, and [`crate::Outcome`] carries
//! them. A start function's are, since a module whose start function does not
//! return has no instance to call.

use std::fmt;

use spacewasm::{
    AllocError, HostFunctionError, HostNameError, MemoryError, ParseError, TrapReason, ValType,
};
use thiserror::Error;

use crate::args::{arguments_phrase, type_name};
use crate::fprime::ReferenceHost;
use crate::report::{HostTrap, TrapReport};

/// Why a module did not load.
#[derive(Debug, Error)]
pub enum LoadError {
    /// The module imports something the F´ reference hosts do not provide as
    /// it declares it. [`crate::fprime::load`] checks this before it decodes a
    /// byte, so nothing of the module ran.
    #[error("{0}")]
    UnsupportedImports(UnsupportedImports),

    /// The F´ reference hosts could not be built to load the module against.
    #[error("the F Prime reference hosts could not be built: {0}")]
    Hosts(HostSetError),

    /// The interpreter refused the bytes, with the offset and the reason the
    /// decoder gave. Decoding, validation and IR compilation are one pass, so
    /// [`crate::load`] and [`crate::load_with`] return every verdict about the
    /// module itself as this.
    ///
    /// [`crate::fprime::load`] returns fewer of them as this. It refuses an
    /// import the reference hosts do not provide as
    /// [`LoadError::UnsupportedImports`] before decoding, and of a module the
    /// target's conformance check accepts it reports a refusal for want of
    /// memory as [`LoadError::OverLimit`] or [`LoadError::CodePagesExhausted`],
    /// and a refusal the check should have made as
    /// [`LoadError::ConformanceGap`]. What is left as this is a module the
    /// check refuses too, and the verdicts the check leaves to others: the
    /// host set's; every other allocation that failed; a `memory.grow`, which
    /// the code builder refuses because the load tells it to, as
    /// `spacewasm_std` tells its own; and the three about the interpreter's
    /// compiled form of the module — `LabelJumpTooLarge`, `PageFault` and
    /// `PossibleBackpatchCycle` — which the check cannot reproduce without
    /// being the interpreter.
    #[error(
        "the module does not load under the SpaceWasm interpreter: {:?} at byte {}",
        .0.err.err,
        .0.offset
    )]
    Decode(ParseError),

    /// The decoder refused the module for want of memory, and the module
    /// needs more of one of the verifier's two stacks than the interpreter was
    /// built with.
    ///
    /// The interpreter gives one verdict, `AllocError(OutOfMemory)`, for three
    /// causes — too many control frames, too tall an operand stack, too many IR
    /// code pages — and names none; the target's conformance check, run again
    /// on the same bytes, measures the two stacks and names the function.
    #[error("the module does not load under the SpaceWasm interpreter: its {0}")]
    OverLimit(OverLimit),

    /// The decoder refused the module for want of memory, and its control
    /// nesting and operand stack are within the verifier's bounds, which
    /// leaves the third cause: its compiled form does not fit the IR code
    /// pages the code builder was given. [`code_pages_exhausted`] words why,
    /// naming the program that loaded it.
    #[error(
        "the module does not load under the SpaceWasm interpreter: {}",
        code_pages_exhausted(*.pages, .verdict, "the runner")
    )]
    CodePagesExhausted {
        /// The IR code pages the code builder was given.
        pages: usize,
        /// The decoder's own verdict.
        verdict: ParseError,
    },

    /// The decoder refused a module the target's conformance check accepts,
    /// for a reason the check does not leave to others: something in the
    /// bytes it should have caught and did not, such as a data segment that
    /// does not fit linear memory or starts at a negative offset.
    ///
    /// A verdict the check leaves to the embedder or cannot reproduce is
    /// never this, since the check's maintainers could do nothing with a
    /// report of it; [`LoadError::Decode`] lists them. [`conformance_gap`]
    /// words why.
    #[error(
        "the module does not load under the SpaceWasm interpreter: {}",
        conformance_gap(.0)
    )]
    ConformanceGap(ParseError),

    /// The interpreter could not be built to load it: an allocation failed,
    /// or the configuration asks for more IR pages than the code builder
    /// addresses.
    #[error("the SpaceWasm interpreter could not allocate {part}: {error:?}")]
    Resource {
        /// What was being built.
        part: &'static str,
        /// The interpreter's own error.
        error: MemoryError,
    },
}

/// Why a loaded module's start function did not return, which leaves the
/// module without an instance to call.
///
/// Only [`crate::LoadedModule::start`] gives one: a load runs nothing, so no
/// [`LoadError`] is about the start function.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StartError {
    /// The start function trapped. A start function the engine refuses to
    /// begin because its frame does not fit the value stack is one of these,
    /// with `StackOverflow`, as a call is.
    #[error("the module's start function trapped: {0:?}")]
    Trapped(TrapReason),

    /// The start function was still running when its instruction budget ran
    /// out.
    #[error("the module's start function was still running after {budget} instructions")]
    OutOfFuel {
        /// The budget it was given.
        budget: usize,
    },

    /// A host function the start function called paused it, and the runner
    /// never resumes a paused call.
    #[error(
        "the module's start function paused: a host function it called returned \
         `HostFunctionBreak::Pause`, and the runner never resumes a paused call"
    )]
    Paused,

    /// The engine refused to begin the start function, for a reason other
    /// than the stack it needs, which is [`StartError::Trapped`] with
    /// `StackOverflow`.
    ///
    /// No module that passed validation reaches this, since none of the
    /// engine's other three refusals can meet a start function. Validation
    /// holds a start function to the type `[] -> []` and the runner begins it
    /// with no arguments, so their count (`ParamLenMismatch`) and their types
    /// (`ParamTypeMismatch`) always match; and the engine was built for this
    /// module, which runs nothing before its start function, so it is idle
    /// (`Busy`). It is a value rather than a panic so that an interpreter
    /// defect is reported by name.
    #[error("the SpaceWasm interpreter refused to invoke the module's start function: {refusal}")]
    Refused {
        /// The interpreter's refusal, spelled as its `Debug` form spells it.
        refusal: String,
    },
}

/// Why [`crate::fprime::HostedModule::start`] returned no instance: the
/// [`StartError`], with what a report of it needs.
///
/// A host that stops the program records why before it traps, and the detail
/// is read from the module's hosts, which go with the module when a start
/// function does not return. So the detail is kept here, beside the stack the
/// module was loaded with, and [`HostedStartError::trap_report`] words a start
/// function's trap as specifically as
/// [`crate::fprime::HostedInstance::trap_report`] words a call's.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{error}")]
pub struct HostedStartError {
    error: StartError,
    host_trap: Option<HostTrap>,
    stack_words: usize,
}

impl HostedStartError {
    /// The failure, with the detail its hosts recorded and the words of value
    /// stack the module was loaded with.
    pub(crate) fn new(error: StartError, host_trap: Option<HostTrap>, stack_words: usize) -> Self {
        Self { error, host_trap, stack_words }
    }

    /// Why the start function did not return.
    #[must_use]
    pub fn error(&self) -> &StartError {
        &self.error
    }

    /// Why a host stopped the start function, when one did.
    #[must_use]
    pub fn host_trap(&self) -> Option<HostTrap> {
        self.host_trap
    }

    /// The report of the start function's trap, with the host's detail when a
    /// host stopped it and the stack the module was loaded with; `None` when
    /// the start function did not trap but ran out of fuel, paused, or was
    /// refused.
    #[must_use]
    pub fn trap_report(&self) -> Option<TrapReport> {
        match self.error {
            StartError::Trapped(reason) => {
                Some(TrapReport::start_function(reason, self.host_trap, self.stack_words))
            }
            StartError::OutOfFuel { .. } | StartError::Paused | StartError::Refused { .. } => None,
        }
    }
}

/// Why a loaded module's function was not called, or a call to it did not end
/// as its signature says.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvokeError {
    /// The module exports nothing by that name, or exports it as a function
    /// index the decoder did not resolve.
    #[error("the module exports no function named `{export}`; {}", exports_clause(.exports))]
    NoSuchExport {
        /// The name asked for.
        export: String,
        /// The functions the module does export, in export-section order.
        exports: Vec<String>,
    },

    /// The name is exported as a memory, a table or a global.
    #[error("`{export}` is exported, but it is {}, not a function", .kind.description())]
    NotAFunction {
        /// The name asked for.
        export: String,
        /// What the module exports under it.
        kind: ExportKind,
    },

    /// The name is exported, and is a host import the module exports again.
    #[error(
        "`{export}` resolves to the host import `{import}`: the interpreter invokes only \
         functions with a WebAssembly body, and a host import's body is the embedder's, so \
         invoke an export that calls it instead"
    )]
    HostReexport {
        /// The name asked for.
        export: String,
        /// The host import behind it, as `module.field`.
        import: String,
    },

    /// The call passed a different number of arguments than the function
    /// declares.
    #[error(
        "`{export}` takes {}; {} given",
        arguments_phrase(*.expected),
        arguments_phrase(*.given)
    )]
    Arity {
        /// The function called.
        export: String,
        /// The parameters it declares.
        expected: usize,
        /// The arguments passed.
        given: usize,
    },

    /// The call passed the right number of arguments, and some of them have
    /// another type than the parameter they fill.
    #[error("`{export}` takes ({}), and was called with ({})", types(.expected), types(.given))]
    Argument {
        /// The function called.
        export: String,
        /// The types of its parameters.
        expected: Vec<ValType>,
        /// The types of the arguments passed.
        given: Vec<ValType>,
    },

    /// The engine is still running an earlier call.
    #[error("`{export}` could not be invoked: the interpreter is still running an earlier call")]
    Busy {
        /// The function called.
        export: String,
    },

    /// A host function the call reached paused it, and the runner never
    /// resumes a paused call. The call is abandoned and the engine is idle
    /// again, so the module can be called once more.
    #[error(
        "`{export}` paused: a registered host function returned `HostFunctionBreak::Pause`, and \
         the runner never resumes a paused call"
    )]
    Paused {
        /// The function called.
        export: String,
    },

    /// The call finished, and the function declares a result the interpreter
    /// did not leave.
    ///
    /// Only an interpreter defect reaches this: validation holds every function
    /// body to leaving exactly the result its type declares. It is a value so
    /// that such a defect is reported by name rather than read as a call that
    /// returned nothing.
    #[error(
        "`{export}` declares a result, and the SpaceWasm interpreter finished the call without \
         one"
    )]
    NoResult {
        /// The function called.
        export: String,
    },
}

/// Why a host module or a host set could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostSetError {
    /// The interpreter will not register a host module or a host function
    /// under this name: it is longer than such a name holds.
    #[error("the SpaceWasm interpreter refuses the host name `{name}` ({error:?})")]
    Name {
        /// The name refused.
        name: String,
        /// The interpreter's own error.
        error: HostNameError,
    },

    /// The interpreter will not build a host function at this signature, or
    /// its allocator cannot hold the function.
    #[error("the SpaceWasm interpreter refuses the host function `{name}` ({error:?})")]
    Function {
        /// The function refused, as `module.field`.
        name: String,
        /// The interpreter's own error.
        error: HostFunctionError,
    },

    /// The interpreter's allocator could not hold the list.
    #[error("the SpaceWasm interpreter's allocator cannot hold a host list: {0:?}")]
    Allocation(AllocError),
}

/// Why an argument written as text is not a value of the parameter it fills.
///
/// Each text is a clause a caller ends as it ends its own sentences.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArgumentError {
    /// A different number of arguments than the function declares.
    #[error("{}, and {}", arity_clause(.function, .params), given_phrase(.given))]
    Count {
        /// The function called.
        function: String,
        /// The types of the parameters it declares.
        params: Vec<ValType>,
        /// The arguments, as written.
        given: Vec<String>,
    },

    /// An integer parameter's argument is not a decimal integer at all.
    #[error(
        "argument {position} for `{function}` is `{text}`, which is not a decimal integer; \
         `{function}` takes ({})",
        types(.params)
    )]
    NotAnInteger {
        /// The function called.
        function: String,
        /// The argument's position, counted from one.
        position: usize,
        /// The types of the parameters the function declares.
        params: Vec<ValType>,
        /// The argument as written.
        text: String,
    },

    /// A decimal integer outside what its parameter's width holds, read as
    /// either a signed number or an unsigned bit pattern.
    #[error(
        "argument {position} for `{function}` is {text}, which does not fit an {} ({})",
        type_name(*.ty),
        accepted_range(*.ty)
    )]
    OutOfRange {
        /// The function called.
        function: String,
        /// The argument's position, counted from one.
        position: usize,
        /// The parameter's type.
        ty: ValType,
        /// The argument as written.
        text: String,
    },

    /// A floating-point parameter, which no argument written as text fills.
    #[error("{}", floating_point_clause(.function, *.ty, "the runner"))]
    FloatingPoint {
        /// The function called.
        function: String,
        /// The argument's position, counted from one.
        position: usize,
        /// The parameter's type.
        ty: ValType,
    },
}

impl ArgumentError {
    /// The refusal as a clause, naming `embedder` where it names the program
    /// passing the arguments, written as it should appear, backticks
    /// included. Only a floating-point parameter's refusal names one — "`f`
    /// takes an f32 argument, which `infs run` cannot pass" — and its text
    /// names the runner; every other refusal reads as its text.
    #[must_use]
    pub fn clause(&self, embedder: &str) -> String {
        match self {
            ArgumentError::FloatingPoint { function, ty, .. } => {
                floating_point_clause(function, *ty, embedder)
            }
            ArgumentError::Count { .. }
            | ArgumentError::NotAnInteger { .. }
            | ArgumentError::OutOfRange { .. } => self.to_string(),
        }
    }
}

/// The imports of a module that the F´ reference hosts do not provide as the
/// module declares them, sorted by module and then field name.
///
/// Built only by [`crate::fprime::check_imports`], and never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedImports {
    imports: Vec<UnsupportedImport>,
}

impl UnsupportedImports {
    /// Sorts `imports`, or answers `None` for a module with nothing to refuse.
    pub(crate) fn of(mut imports: Vec<UnsupportedImport>) -> Option<Self> {
        if imports.is_empty() {
            return None;
        }
        imports.sort_by(|a, b| (&a.module, &a.field).cmp(&(&b.module, &b.field)));
        Some(Self { imports })
    }

    /// Every import refused, sorted by module and then field name.
    #[must_use]
    pub fn imports(&self) -> &[UnsupportedImport] {
        &self.imports
    }
}

impl fmt::Display for UnsupportedImports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.imports.len();
        let imports = if count == 1 { "import" } else { "imports" };
        write!(
            f,
            "the module has {count} {imports} that the F Prime reference hosts do not provide as \
             declared:"
        )?;
        for line in self.import_lines("the F Prime reference set") {
            write!(f, "\n{line}")?;
        }
        Ok(())
    }
}

impl std::error::Error for UnsupportedImports {}

/// One import the F´ reference hosts do not provide as the module declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedImport {
    /// The module name the import is declared under.
    pub module: String,
    /// The field name the import is declared under.
    pub field: String,
    /// What is wrong with it.
    pub problem: ImportProblem,
}

/// Why the F´ reference hosts do not provide one import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportProblem {
    /// No reference host has both of the import's names. `elsewhere` is the
    /// reference host with the same field under the other module name, when
    /// there is one: the import's author most likely meant that one.
    Unknown {
        /// The host with the same field under another module.
        elsewhere: Option<&'static ReferenceHost>,
    },
    /// A reference host has both of the import's names, and another
    /// signature. The module's side is in WebAssembly's type names, which is
    /// all the artifact records.
    Mismatch {
        /// The parameter types the module declares.
        params: Vec<String>,
        /// The result types the module declares.
        results: Vec<String>,
        /// The host the names belong to.
        host: &'static ReferenceHost,
    },
    /// The import is not a function: a `memory`, a `table`, a `global` or a
    /// `tag`, which this word names.
    NotAFunction {
        /// What the import is.
        kind: &'static str,
    },
}

/// What a module exports under a name that is not a function's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// Its linear memory.
    Memory,
    /// Its table.
    Table,
    /// One of its globals.
    Global,
}

impl ExportKind {
    /// What the export is, in the words a refusal to call it uses: "the
    /// module's linear memory", "the module's table" or "one of the module's
    /// globals".
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            ExportKind::Memory => "the module's linear memory",
            ExportKind::Table => "the module's table",
            ExportKind::Global => "one of the module's globals",
        }
    }
}

/// Which of the verifier's two stacks a module needs more of than the
/// interpreter was built with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Nested control frames, the function body's own included.
    ControlFrames,
    /// Operand-stack values, whatever their width.
    OperandStack,
}

impl Limit {
    /// The name of the embedder's const generic that bounds this stack.
    #[must_use]
    pub fn const_generic(self) -> &'static str {
        match self {
            Limit::ControlFrames => "MAX_CONTROL_FRAMES",
            Limit::OperandStack => "MAX_STACK_DEPTH",
        }
    }
}

/// A module that needs more of one verifier stack than the interpreter was
/// built with, as the conformance check measured it.
#[derive(Debug, Clone)]
pub struct OverLimit {
    /// The stack the module needs more of.
    pub limit: Limit,
    /// The function that needs the most of it.
    pub function: String,
    /// How much of it that function needs.
    pub needed: u32,
    /// How much of it the interpreter was built with.
    pub allowed: u32,
    /// The decoder's own verdict, `AllocError(OutOfMemory)`.
    pub verdict: ParseError,
}

impl OverLimit {
    /// The measurement in words: "control nesting of 71 frames (in
    /// `render_row`) exceeds the 64", or "tallest operand stack of 300 values
    /// (in `mix`) exceeds the 256".
    #[must_use]
    pub fn fact(&self) -> String {
        let (quantity, unit) = match self.limit {
            Limit::ControlFrames => ("control nesting", "frames"),
            Limit::OperandStack => ("tallest operand stack", "values"),
        };
        format!(
            "{quantity} of {} {unit} (in `{}`) exceeds the {}",
            self.needed, self.function, self.allowed
        )
    }
}

impl fmt::Display for OverLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} the interpreter's verifier was built with", self.fact())
    }
}

/// Why a module did not load whose compiled form does not fit the `pages` IR
/// code pages `embedder` gave the code builder, with the decoder's `verdict`,
/// in the words [`LoadError::CodePagesExhausted`] is reported in: the compiled
/// form does not fit "the 256 IR code pages `infs run` provides", the decoder
/// reported `AllocError(OutOfMemory)` at the byte `verdict` names, and the
/// control nesting and the operand stack are "within the limits `infs run`
/// loads it at".
///
/// `embedder` names the program that loaded the module, written as it should
/// appear, backticks included; the variant's own text names the runner.
#[must_use]
pub fn code_pages_exhausted(pages: usize, verdict: &ParseError, embedder: &str) -> String {
    format!(
        "the interpreter's compiled form of it does not fit the {} {embedder} provides (the \
         interpreter reported AllocError(OutOfMemory) at byte {}); its control nesting and \
         operand stack are within the limits {embedder} loads it at",
        code_pages(pages),
        verdict.offset
    )
}

/// Why a module the target's conformance check accepts did not load, when the
/// decoder's `verdict` is one the check should have given too, in the words
/// [`LoadError::ConformanceGap`] is reported in: the verdict and the byte it
/// names, that infc's conformance check accepts the module and so has a gap,
/// and where to report it.
///
/// Unlike [`code_pages_exhausted`] it names no embedder: the gap is the
/// check's, whichever program loaded the module.
#[must_use]
pub fn conformance_gap(verdict: &ParseError) -> String {
    format!(
        "{:?} at byte {}. infc's conformance check accepts this module, so this is a gap in that \
         check; please report it at https://github.com/Inferara/inference/issues with the artifact",
        verdict.err.err, verdict.offset
    )
}

/// "it exports `a`, `b`" or "it exports no functions".
fn exports_clause(exports: &[String]) -> String {
    if exports.is_empty() {
        return "it exports no functions".to_string();
    }
    let names: Vec<String> = exports.iter().map(|name| format!("`{name}`")).collect();
    format!("it exports {}", names.join(", "))
}

/// Value types in the WebAssembly spelling, separated by commas.
fn types(types: &[ValType]) -> String {
    types.iter().map(|ty| type_name(*ty)).collect::<Vec<_>>().join(", ")
}

/// "`main` takes no arguments", "`echo` takes 1 argument (i64)" or "`add`
/// takes 2 arguments (i32, i32)".
pub(crate) fn arity_clause(function: &str, params: &[ValType]) -> String {
    format!("`{function}` takes {}", parameters_phrase(params))
}

/// "no arguments", "1 argument (i32)" or "2 arguments (i32, i64)".
fn parameters_phrase(params: &[ValType]) -> String {
    match params.len() {
        0 => "no arguments".to_string(),
        count => format!("{} ({})", arguments_phrase(count), types(params)),
    }
}

/// "none were given", "1 was given: 5" or "3 were given: 2 40 7".
fn given_phrase(given: &[String]) -> String {
    match given.len() {
        0 => "none were given".to_string(),
        1 => format!("1 was given: {}", given.join(" ")),
        count => format!("{count} were given: {}", given.join(" ")),
    }
}

/// The decimal integers an argument of `ty` may be written as: the signed
/// range and the unsigned one together, a value above the signed maximum
/// being the bit pattern of a negative number.
fn accepted_range(ty: ValType) -> String {
    let (min, signed_max, unsigned_max) = match ty {
        ValType::I32 => (i128::from(i32::MIN), i128::from(i32::MAX), i128::from(u32::MAX)),
        ValType::I64 => (i128::from(i64::MIN), i128::from(i64::MAX), i128::from(u64::MAX)),
        ValType::F32 | ValType::F64 => return "no decimal integer".to_string(),
    };
    format!(
        "{min} to {unsigned_max}; a value above {signed_max} is taken as its unsigned bit pattern"
    )
}

/// "1 IR code page" or "256 IR code pages".
fn code_pages(count: usize) -> String {
    if count == 1 { "1 IR code page".to_string() } else { format!("{count} IR code pages") }
}

/// "`f` takes an f32 argument, which `infs run` cannot pass".
fn floating_point_clause(function: &str, ty: ValType, embedder: &str) -> String {
    format!("`{function}` takes an {} argument, which {embedder} cannot pass", type_name(ty))
}
