//! Compiler interface version constants shared by `infs` and `infc`.
//!
//! The ABI (application binary interface) here means the set of CLI flags,
//! stdin/stdout contract, and exit codes that `infs` relies on when
//! invoking `infc` as a subprocess. Bump the major on any breaking change;
//! bump the minor on additive, backward-compatible changes.
//!
//! Single source of truth: [`COMPILER_ABI_MAJOR`] and [`COMPILER_ABI_MINOR`].
//! Callers that need a `<major>.<minor>` string format it with those
//! constants directly — keeping the numeric and string forms from drifting.

/// Breaking ABI changes: incompatible CLI flag removal/rename, stdout contract
/// changes, exit-code semantics changes.
pub const COMPILER_ABI_MAJOR: u32 = 1;

/// Additive changes: new flags, new stdout fields, new exit codes.
pub const COMPILER_ABI_MINOR: u32 = 0;
