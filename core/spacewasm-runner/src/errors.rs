//! Every way loading a module, calling one of its functions, building a host
//! set or reading an argument can fail.
//!
//! A trap and an exhausted instruction budget are not here: they are how a
//! call ends, not a failure to make it, and [`crate::Outcome`] carries them.

use spacewasm::{AllocError, HostNameError, MemoryError, ParseError, TrapReason, ValType};
use thiserror::Error;

use crate::args::{arguments_phrase, type_name};

/// Why a module did not load.
#[derive(Debug, Error)]
pub enum LoadError {
    /// The interpreter refused the bytes. Decoding, validation and IR
    /// compilation are one pass, so this is every verdict about the module
    /// itself, carrying the offset and the reason the decoder gave.
    #[error(
        "the module does not load under the SpaceWasm interpreter: {:?} at byte {}",
        .0.err.err,
        .0.offset
    )]
    Decode(ParseError),

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

    /// The module's start function trapped.
    #[error("the module's start function trapped: {0:?}")]
    StartTrapped(TrapReason),

    /// The module's start function was still running when its instruction
    /// budget ran out.
    #[error("the module's start function was still running after {budget} instructions")]
    StartOutOfFuel {
        /// The budget it was given.
        budget: usize,
    },

    /// A host function the start function called paused it, and the runner
    /// never resumes a paused call.
    #[error(
        "the module's start function paused: a host function it called returned \
         `HostFunctionBreak::Pause`, and the runner never resumes a paused call"
    )]
    StartPaused,

    /// The engine refused to begin the start function, for a reason other
    /// than the stack it needs, which is [`LoadError::StartTrapped`] with
    /// `StackOverflow`.
    ///
    /// No module that passed validation reaches this, since none of the
    /// engine's other three refusals can meet a start function. Validation
    /// holds a start function to the type `[] -> []` and the runner begins it
    /// with no arguments, so their count (`ParamLenMismatch`) and their types
    /// (`ParamTypeMismatch`) always match; and the engine was built for this
    /// load and has run nothing before it, so it is idle (`Busy`). It is a
    /// value rather than a panic so that an interpreter defect is reported by
    /// name.
    #[error("the SpaceWasm interpreter refused to invoke the module's start function: {refusal}")]
    StartRefused {
        /// The interpreter's refusal, spelled as its `Debug` form spells it.
        refusal: String,
    },
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
    #[error("`{export}` is exported, but not as a function")]
    NotAFunction {
        /// The name asked for.
        export: String,
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
    /// The interpreter will not register a host module under this name: it is
    /// longer than a host module name holds.
    #[error("the SpaceWasm interpreter refuses the host module name `{name}` ({error:?})")]
    Name {
        /// The name refused.
        name: String,
        /// The interpreter's own error.
        error: HostNameError,
    },

    /// The interpreter's allocator could not hold the list.
    #[error("the SpaceWasm interpreter's allocator cannot hold a host list: {0:?}")]
    Allocation(AllocError),
}

/// Why an argument written as text is not a value of the parameter it fills.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArgumentError {
    /// A different number of arguments than the function declares.
    #[error(
        "`{function}` takes {}; {} given",
        arguments_phrase(*.expected),
        arguments_phrase(*.given)
    )]
    Count {
        /// The function called.
        function: String,
        /// The parameters it declares.
        expected: usize,
        /// The arguments written.
        given: usize,
    },

    /// An integer parameter's argument is not a decimal integer of its width.
    #[error(
        "argument {position} of `{function}` is `{}`, and `{text}` is not one",
        type_name(*.ty)
    )]
    NotAnInteger {
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
    #[error(
        "argument {position} of `{function}` is `{}`, and this harness passes decimal integers \
         only: how a floating-point argument should be spelled on a command line is a question \
         it does not have to settle",
        type_name(*.ty)
    )]
    FloatingPoint {
        /// The function called.
        function: String,
        /// The argument's position, counted from one.
        position: usize,
        /// The parameter's type.
        ty: ValType,
    },
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
