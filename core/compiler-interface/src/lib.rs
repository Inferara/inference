//! Compiler interface version constants shared by `infs` and `infc`.
//!
//! The ABI (application binary interface) here means the set of CLI flags,
//! stdin/stdout contract, and exit codes that `infs` relies on when
//! invoking `infc` as a subprocess. Bump the major on any breaking change;
//! bump the minor on additive, backward-compatible changes.

/// Breaking ABI changes: incompatible CLI flag removal/rename, stdout contract
/// changes, exit-code semantics changes.
pub const COMPILER_ABI_MAJOR: u32 = 1;

/// Additive changes: new flags, new stdout fields, new exit codes.
pub const COMPILER_ABI_MINOR: u32 = 0;

/// Returns the version as `"<major>.<minor>"`.
#[must_use]
pub const fn abi_version_string() -> &'static str {
    // const fn can't call format! — keep in sync with the constants above.
    "1.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_string_matches_constants() {
        let expected = format!("{COMPILER_ABI_MAJOR}.{COMPILER_ABI_MINOR}");
        assert_eq!(abi_version_string(), expected);
    }
}
