use thiserror::Error;

/// Error returned when a function call expression cannot be lowered by the codegen pass.
///
/// This is an internal error type used by [`super::compiler::Compiler::lower_function_call`].
/// Callers convert it to a `todo!` or `panic!` depending on whether the case is planned
/// future work or indicates a type-checker inconsistency.
#[derive(Debug, Error)]
#[must_use = "errors must not be silently ignored"]
pub(crate) enum CodegenError {
    /// The callee expression is not a plain identifier (e.g., method call, higher-order call).
    /// This case is out of scope for the current implementation.
    #[error("unsupported callee kind (only plain identifier calls are currently supported)")]
    UnsupportedCalleeKind,
    /// The function name was not found in the pre-built index map.
    /// This should never happen if the type-checker ran successfully.
    #[error(
        "function '{0}' not found in module — the type-checker should have caught undefined functions"
    )]
    UnknownFunction(String),
}
