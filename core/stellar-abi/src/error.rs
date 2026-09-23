//! Every way the rewrite refuses a module.
//!
//! One variant per refusal, each naming the export it is about and the element
//! that made it inadmissible, because the reader of the message is looking at
//! Inference source and needs to know which declaration to change.

use thiserror::Error;

use crate::meta::STELLAR_ENV_PROTOCOL;
use crate::rewrite::{MAX_EXPORT_NAME_BYTES, MAX_VAL_PARAMETERS};
use crate::spec::MAX_INPUT_NAME_BYTES;

/// Why a module cannot become a Soroban contract.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StellarAbiError {
    /// A contract with no methods can be uploaded and never called.
    #[error("the module exports no function, so the contract would have no method to call")]
    NoExportedFunctions,

    /// An exported function the descriptor does not describe. Shipping it would
    /// mean a method that is reachable by name and fails every call, so the
    /// mismatch is refused rather than carried.
    #[error(
        "the module exports `{export}`, which the export descriptor does not describe, so its \
         parameter and return types are unknown"
    )]
    UnknownExport { export: String },

    /// A described export the module does not have. The linker renames nothing
    /// today, so this means the function was dropped, and the contract would be
    /// silently missing a method.
    #[error(
        "the export descriptor describes `{export}`, which the linked module does not export, so \
         the contract would be missing that method"
    )]
    MissingExport { export: String },

    /// The host turns a method name into a `Symbol`, and the empty string is not
    /// one.
    #[error("an exported function has an empty name, which is not a callable contract method")]
    EmptyExportName,

    /// Over the `Symbol` limit: no caller can express the name, so the method is
    /// unreachable however it is invoked.
    #[error(
        "the export `{export}` is {len} bytes long; a contract method name is at most \
         {MAX_EXPORT_NAME_BYTES} bytes, and a longer one cannot be named by any caller"
    )]
    ExportNameTooLong { export: String, len: usize },

    /// The host reserves the `__` prefix and refuses to dispatch to it.
    #[error(
        "the export `{export}` starts with `__`, a prefix the host reserves: the method uploads \
         and then refuses every call with `can't invoke a reserved function directly`"
    )]
    ReservedExportName { export: String },

    /// Outside `[A-Za-z0-9_]`, so the name is not a `Symbol` and no caller can
    /// express it.
    #[error(
        "the export `{export}` contains `{offending}`; a contract method name may hold only \
         letters, digits and `_`, and a name with anything else cannot be reached by any caller"
    )]
    ExportNameNotASymbol { export: String, offending: char },

    /// Past the host's argument-count limit.
    #[error(
        "the export `{export}` takes {count} parameters; a contract method takes at most \
         {MAX_VAL_PARAMETERS}"
    )]
    TooManyParameters { export: String, count: usize },

    /// A parameter type outside the M1 scalar set.
    #[error(
        "the export `{export}` takes `{ty}` at parameter {position}; a contract method parameter \
         must be `u32`, `i32` or `bool`"
    )]
    UnsupportedParameter {
        export: String,
        /// One-based, counting the parameters as the source declared them.
        ///
        /// A user can reach this refusal and the source-level gate's for the
        /// same mistake — the gate refuses what it can see, this pass refuses
        /// what reaches it — and two logs numbering the same parameter
        /// differently would read as two different defects.
        position: usize,
        ty: String,
    },

    /// A return type outside the M1 scalar set.
    #[error(
        "the export `{export}` returns `{ty}`; a contract method must return `u32`, `i32`, \
         `bool` or nothing"
    )]
    UnsupportedReturn { export: String, ty: String },

    /// A parameter the source wrote as `_`.
    ///
    /// The contract spec records every parameter by name, and that name is how
    /// a caller reaches it: `stellar contract invoke` passes each argument as
    /// `--<name>`. A parameter with none cannot be described, and inventing one
    /// would publish a name the author never wrote.
    #[error(
        "the export `{export}` leaves parameter {position} unnamed, written `_`; a contract \
         method's parameters are named, because `stellar contract invoke` passes each one as \
         `--<name>` and the contract spec section records that name, so name the parameter"
    )]
    UnnamedParameter {
        export: String,
        /// One-based, as [`StellarAbiError::UnsupportedParameter`] counts.
        position: usize,
    },

    /// A parameter name wider than the field the contract spec records it in.
    #[error(
        "the export `{export}` names parameter {position} `{name}`, which is {len} bytes long; \
         a contract method's parameter name is at most {MAX_INPUT_NAME_BYTES} bytes, the width \
         of the contract spec section's input-name field, so shorten it"
    )]
    ParameterNameTooLong {
        export: String,
        /// One-based, as [`StellarAbiError::UnsupportedParameter`] counts.
        position: usize,
        name: String,
        len: usize,
    },

    /// A struct or array return, which is passed through a hidden pointer into
    /// linear memory and has no `Val` counterpart.
    #[error(
        "the export `{export}` returns the compound type `{ty}` through a pointer into linear \
         memory, which a contract method cannot give back to a caller"
    )]
    CompoundReturn { export: String, ty: String },

    /// The descriptor and the module disagree about an export's WebAssembly
    /// signature, which means one of them is describing a different function.
    #[error(
        "the export `{export}` is described as `{expected}` but the module declares it as \
         `{found}`, so the descriptor and the module disagree about what it is"
    )]
    DescriptorDisagreesWithModule {
        export: String,
        expected: String,
        found: String,
    },

    /// A surviving import. Every host function a contract may import is a
    /// separate design question, and the convention behind them all is unbound
    /// at this target.
    #[error(
        "the module imports `{module}::{name}`; a contract may import only host functions, and \
         such an import is refused at this target until the Soroban host-call convention is \
         bound"
    )]
    ImportsUnsupported { module: String, name: String },

    /// A start function, which would run at instantiation, outside any call.
    #[error(
        "the module declares function {function} as its start function; a contract is entered \
         only through a method call"
    )]
    StartSectionPresent { function: u32 },

    /// The module already carries one of the three sections this pass writes —
    /// `contractspecv0`, `contractmetav0` or `contractenvmetav0` — so it has
    /// already been made a contract. Rewriting it again would wrap the wrappers
    /// and write that section a second time, and a second copy is not merely
    /// redundant. A spec reader either takes the first `contractspecv0` it
    /// meets, as `soroban_spec::read::raw_from_wasm` (`soroban-spec` 28.0.0)
    /// does, or merges every copy, and neither describes the contract this pass
    /// produced.
    #[error(
        "the module already carries a `{section}` section, which this pass writes, so it has \
         already been made a contract"
    )]
    AlreadyAContract { section: String },

    /// A declared environment protocol older than the one Soroban arrived in.
    /// The section it would produce is well formed, so nothing refuses it until
    /// upload, where every host that has ever existed refuses it.
    #[error(
        "the declared environment protocol {protocol} predates Soroban, which arrived in \
         protocol {STELLAR_ENV_PROTOCOL}; a contract declaring it is well formed and no host \
         has ever accepted one"
    )]
    ProtocolPredatesSoroban { protocol: u32 },

    /// More than one `name` custom section. WebAssembly permits repeated custom
    /// sections and no validator rejects them, but this pass extends exactly one
    /// function-name map and has no way to say which of two a reader would
    /// believe.
    #[error(
        "the module carries more than one `name` custom section; this pass extends exactly one \
         function-name map and cannot tell which of them names the module's functions"
    )]
    MultipleNameSections,

    /// The bytes are not a WebAssembly module this pass can read.
    #[error("the module could not be parsed: {reason}")]
    MalformedModule { reason: String },

    /// The module uses something outside WebAssembly 1.0, which is the dialect a
    /// Soroban host runs.
    #[error("the module is not a WebAssembly 1.0 module, which a contract must be: {reason}")]
    InputNotWasm1 { reason: String },

    /// The rewrite produced a module that does not validate. Nothing is written:
    /// this is a defect in this pass, reported rather than shipped.
    #[error("the Val ABI rewrite produced a module that does not validate: {reason}")]
    RewrittenNotWasm1 { reason: String },
}
