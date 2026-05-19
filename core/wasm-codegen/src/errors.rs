use thiserror::Error;

/// Error returned when a function call expression cannot be lowered by the codegen pass.
///
/// This is an internal error type used by [`super::compiler::Compiler::lower_function_call`]
/// and sret return lowering. Callers convert it to a `panic!` depending on whether the
/// case indicates a type-checker inconsistency.
#[derive(Debug, Error)]
#[must_use = "errors must not be silently ignored"]
pub(crate) enum CodegenError {
    /// The function name was not found in the pre-built index map.
    /// This should never happen if the type-checker ran successfully.
    #[error(
        "function '{0}' not found in module — the type-checker should have caught undefined functions"
    )]
    UnknownFunction(String),
    /// The return expression in an sret function is not a supported form.
    /// Supported forms: identifier, array literal, or call to another sret function.
    #[error(
        "unsupported sret return expression in function — expected identifier, array literal, or array-returning function call"
    )]
    UnsupportedSretReturnExpression,
    /// An array (or nested array) has too many total elements for uzumaki
    /// unrolling. Each element produces several WASM instructions, so
    /// unbounded unrolling leads to O(n) instruction explosion.
    #[error(
        "array has {total_elements} elements which exceeds the maximum of {max} for uzumaki unrolling"
    )]
    ArrayTooLargeForUzumaki { total_elements: u32, max: u32 },
    /// Cycle detected in struct layout computation. The type checker should
    /// prevent recursive struct definitions, so this variant is defense-in-depth.
    #[error("cycle detected in struct layout for '{name}' -- the struct transitively contains itself")]
    CycleInStructLayout { name: String },
    /// A struct name referenced during layout computation was not found in the type context.
    #[error("struct '{name}' not found in type context -- the type checker should have caught this")]
    StructNotFoundInTypeContext { name: String },
    /// A `spec` block contained another `spec` block. Nested specs have no
    /// defined Rocq emission; the codegen pipeline refuses rather than
    /// silently dropping the inner definitions.
    #[error("nested specs are not supported: spec '{outer_spec}' contains spec '{inner_spec}'")]
    NestedSpecsNotSupported {
        outer_spec: String,
        inner_spec: String,
    },
}
