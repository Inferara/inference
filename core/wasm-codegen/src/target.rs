//! Compilation target and mode definitions for the Inference compiler.
//!
//! This module defines the target platform, compilation mode, and optimization level
//! types used throughout the code generation pipeline. These types control how LLVM IR
//! is generated and how external tools (`inf-llc`, `rust-lld`) are invoked.
//!
//! # Target
//!
//! The [`Target`] enum specifies the WebAssembly target platform:
//! - [`Target::Wasm32`] — General-purpose WASM using custom `inf-llc` with Inference intrinsic
//!   support. Used for both verification (`proof` mode) and general execution (`compile` mode).
//! - [`Target::Soroban`] — Stellar Soroban smart contract target for standard code without
//!   non-deterministic instructions.
//!
//! # Compilation Mode
//!
//! The [`CompilationMode`] enum controls optimization and spec-node handling:
//! - [`CompilationMode::Compile`] — Produces optimized production binaries. Spec nodes are
//!   stripped from codegen since they have no runtime meaning.
//! - [`CompilationMode::Proof`] — Produces literal, unoptimized WASM that preserves 1:1
//!   structural correspondence with the source code for Rocq formalization.
//!
//! # Optimization Level
//!
//! The [`OptLevel`] enum represents LLVM optimization levels. Since `llc` only accepts
//! `-O0` through `-O3`, size optimization (`Os`/`Oz`) is achieved through IR function
//! attributes (`optsize`/`minsize`) combined with `llc -O2`.

#![allow(clippy::doc_markdown)]

/// Compilation target for code generation.
///
/// Both targets use the same LLVM triple (`wasm32-unknown-unknown`) and CPU (`mvp`),
/// but differ in enabled WebAssembly features and linker flags.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::Target;
///
/// let target = Target::default();
/// assert_eq!(target, Target::Wasm32);
/// assert_eq!(target.triple(), "wasm32-unknown-unknown");
/// assert_eq!(target.cpu(), "mvp");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    /// General-purpose WebAssembly target using strict MVP baseline.
    ///
    /// Uses custom `inf-llc` with Inference intrinsic support for non-deterministic
    /// operations. No post-MVP features are enabled, ensuring compatibility with
    /// the custom 0xfc prefix instruction space.
    ///
    /// Used in both `compile` and `proof` modes.
    #[default]
    Wasm32,

    /// Stellar Soroban smart contract target.
    ///
    /// Enables post-MVP features accepted by the Soroban VM (`+mutable-globals`,
    /// `+sign-ext`, `+bulk-memory`). Produces size-optimized binaries to fit within
    /// the 64 KB contract size limit.
    ///
    /// Only supports `compile` mode — `proof` mode requires custom intrinsics that
    /// are incompatible with the Soroban VM.
    Soroban,
}

/// Compilation mode controlling optimization and spec-node handling.
///
/// The mode is orthogonal to the target: `compile` mode works with any target,
/// while `proof` mode requires the `Wasm32` target (custom intrinsics need strict
/// MVP and `inf-llc`).
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::CompilationMode;
///
/// let mode = CompilationMode::default();
/// assert_eq!(mode, CompilationMode::Compile);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompilationMode {
    /// Produces optimized production binaries.
    ///
    /// Non-deterministic `spec` nodes are stripped from codegen since they have no
    /// runtime meaning. The target's default optimization level applies (`-O3` for
    /// `Wasm32`, `-Oz` for `Soroban`).
    #[default]
    Compile,

    /// Produces literal, unoptimized WASM for formal verification.
    ///
    /// All code (including non-deterministic intrinsics) is emitted with `-O0` and
    /// per-function `optnone`+`noinline` barriers as defense-in-depth. The output
    /// feeds into the `wasm_to_v` Rocq translation for formal verification.
    ///
    /// The target is always `Wasm32` — custom intrinsics require strict MVP and `inf-llc`.
    Proof,
}

/// Optimization level for LLVM compilation.
///
/// Since `llc` only accepts `-O0` through `-O3`, size optimization (`Os`/`Oz`) is
/// achieved by setting `optsize` and/or `minsize` IR function attributes, then
/// invoking `llc -O2`. This matches how Clang implements `-Os`/`-Oz`.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::OptLevel;
///
/// let level = OptLevel::Oz;
/// assert_eq!(level.as_llc_flag(), "-O2");
/// assert!(level.is_size_optimized());
/// assert!(level.is_min_size());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimization. Preserves 1:1 correspondence with source code.
    O0,
    /// Basic optimization level.
    O1,
    /// Default optimization level.
    #[default]
    O2,
    /// Aggressive optimization for performance.
    O3,
    /// Optimize for size. Maps to `llc -O2` with `optsize` IR attribute.
    Os,
    /// Aggressively optimize for size. Maps to `llc -O2` with `minsize`+`optsize` IR attributes.
    Oz,
}

impl OptLevel {
    /// Returns the flag to pass to `inf-llc`.
    ///
    /// Since `llc` does not accept `-Os` or `-Oz`, those map to `-O2`. Size
    /// optimization is achieved through IR function attributes instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::OptLevel;
    ///
    /// assert_eq!(OptLevel::O0.as_llc_flag(), "-O0");
    /// assert_eq!(OptLevel::O3.as_llc_flag(), "-O3");
    /// assert_eq!(OptLevel::Os.as_llc_flag(), "-O2");
    /// assert_eq!(OptLevel::Oz.as_llc_flag(), "-O2");
    /// ```
    #[must_use]
    pub fn as_llc_flag(&self) -> &'static str {
        match self {
            Self::O0 => "-O0",
            Self::O1 => "-O1",
            Self::O2 | Self::Os | Self::Oz => "-O2",
            Self::O3 => "-O3",
        }
    }

    /// Whether to add the `optsize` IR function attribute.
    ///
    /// When true, LLVM will prefer smaller code size over execution speed.
    /// This is set for both `Os` and `Oz` levels.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::OptLevel;
    ///
    /// assert!(!OptLevel::O3.is_size_optimized());
    /// assert!(OptLevel::Os.is_size_optimized());
    /// assert!(OptLevel::Oz.is_size_optimized());
    /// ```
    #[must_use]
    pub fn is_size_optimized(&self) -> bool {
        matches!(self, Self::Os | Self::Oz)
    }

    /// Whether to add the `minsize` IR function attribute.
    ///
    /// When true, LLVM will aggressively minimize code size, even at the expense
    /// of execution speed. This is only set for the `Oz` level.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::OptLevel;
    ///
    /// assert!(!OptLevel::Os.is_min_size());
    /// assert!(OptLevel::Oz.is_min_size());
    /// ```
    #[must_use]
    pub fn is_min_size(&self) -> bool {
        matches!(self, Self::Oz)
    }
}

impl Target {
    /// Returns the LLVM target triple.
    ///
    /// Both `Wasm32` and `Soroban` use the same LLVM backend triple.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert_eq!(Target::Wasm32.triple(), "wasm32-unknown-unknown");
    /// assert_eq!(Target::Soroban.triple(), "wasm32-unknown-unknown");
    /// ```
    #[must_use]
    pub fn triple(&self) -> &'static str {
        // Both targets use the same LLVM backend
        "wasm32-unknown-unknown"
    }

    /// Returns the CPU target for `inf-llc`.
    ///
    /// Both targets pin to the MVP baseline.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert_eq!(Target::Wasm32.cpu(), "mvp");
    /// assert_eq!(Target::Soroban.cpu(), "mvp");
    /// ```
    #[must_use]
    pub fn cpu(&self) -> &'static str {
        // Both pin to MVP
        "mvp"
    }

    /// Returns the LLVM feature flags for `inf-llc -mattr`.
    ///
    /// `Wasm32` uses strict MVP (no features), while `Soroban` enables the three
    /// post-MVP features accepted by the Soroban VM.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert_eq!(Target::Wasm32.llc_features(), "");
    /// assert_eq!(Target::Soroban.llc_features(), "+mutable-globals,+sign-ext,+bulk-memory");
    /// ```
    #[must_use]
    pub fn llc_features(&self) -> &'static str {
        match self {
            Self::Wasm32 => "",
            Self::Soroban => "+mutable-globals,+sign-ext,+bulk-memory",
        }
    }

    /// Returns the default optimization level for the given compilation mode.
    ///
    /// | Target | Compile | Proof |
    /// |--------|---------|-------|
    /// | Wasm32 | O3      | O0    |
    /// | Soroban| Oz      | O0    |
    ///
    /// In `proof` mode, `-O0` is always used to preserve 1:1 structural correspondence
    /// with the source code. In `compile` mode, the optimization level depends on the
    /// target: `O3` for general-purpose WASM, `Oz` for Soroban (64 KB size limit).
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::{Target, CompilationMode, OptLevel};
    ///
    /// assert_eq!(Target::Wasm32.default_opt_level(CompilationMode::Compile), OptLevel::O3);
    /// assert_eq!(Target::Wasm32.default_opt_level(CompilationMode::Proof), OptLevel::O0);
    /// assert_eq!(Target::Soroban.default_opt_level(CompilationMode::Compile), OptLevel::Oz);
    /// assert_eq!(Target::Soroban.default_opt_level(CompilationMode::Proof), OptLevel::O0);
    /// ```
    #[must_use]
    pub fn default_opt_level(&self, mode: CompilationMode) -> OptLevel {
        match mode {
            CompilationMode::Proof => OptLevel::O0,
            CompilationMode::Compile => match self {
                Self::Wasm32 => OptLevel::O3,
                Self::Soroban => OptLevel::Oz,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_default_is_wasm32() {
        assert_eq!(Target::default(), Target::Wasm32);
    }

    #[test]
    fn compilation_mode_default_is_compile() {
        assert_eq!(CompilationMode::default(), CompilationMode::Compile);
    }

    #[test]
    fn opt_level_default_is_o2() {
        assert_eq!(OptLevel::default(), OptLevel::O2);
    }

    #[test]
    fn target_triple_is_wasm32_for_both() {
        assert_eq!(Target::Wasm32.triple(), "wasm32-unknown-unknown");
        assert_eq!(Target::Soroban.triple(), "wasm32-unknown-unknown");
    }

    #[test]
    fn target_cpu_is_mvp_for_both() {
        assert_eq!(Target::Wasm32.cpu(), "mvp");
        assert_eq!(Target::Soroban.cpu(), "mvp");
    }

    #[test]
    fn wasm32_has_no_llc_features() {
        assert_eq!(Target::Wasm32.llc_features(), "");
    }

    #[test]
    fn soroban_has_expected_llc_features() {
        assert_eq!(
            Target::Soroban.llc_features(),
            "+mutable-globals,+sign-ext,+bulk-memory"
        );
    }

    #[test]
    fn wasm32_compile_mode_uses_o3() {
        assert_eq!(
            Target::Wasm32.default_opt_level(CompilationMode::Compile),
            OptLevel::O3
        );
    }

    #[test]
    fn wasm32_proof_mode_uses_o0() {
        assert_eq!(
            Target::Wasm32.default_opt_level(CompilationMode::Proof),
            OptLevel::O0
        );
    }

    #[test]
    fn soroban_compile_mode_uses_oz() {
        assert_eq!(
            Target::Soroban.default_opt_level(CompilationMode::Compile),
            OptLevel::Oz
        );
    }

    #[test]
    fn soroban_proof_mode_uses_o0() {
        assert_eq!(
            Target::Soroban.default_opt_level(CompilationMode::Proof),
            OptLevel::O0
        );
    }

    #[test]
    fn opt_level_llc_flags() {
        assert_eq!(OptLevel::O0.as_llc_flag(), "-O0");
        assert_eq!(OptLevel::O1.as_llc_flag(), "-O1");
        assert_eq!(OptLevel::O2.as_llc_flag(), "-O2");
        assert_eq!(OptLevel::O3.as_llc_flag(), "-O3");
        assert_eq!(OptLevel::Os.as_llc_flag(), "-O2");
        assert_eq!(OptLevel::Oz.as_llc_flag(), "-O2");
    }

    #[test]
    fn opt_level_size_optimized() {
        assert!(!OptLevel::O0.is_size_optimized());
        assert!(!OptLevel::O1.is_size_optimized());
        assert!(!OptLevel::O2.is_size_optimized());
        assert!(!OptLevel::O3.is_size_optimized());
        assert!(OptLevel::Os.is_size_optimized());
        assert!(OptLevel::Oz.is_size_optimized());
    }

    #[test]
    fn opt_level_min_size() {
        assert!(!OptLevel::O0.is_min_size());
        assert!(!OptLevel::O1.is_min_size());
        assert!(!OptLevel::O2.is_min_size());
        assert!(!OptLevel::O3.is_min_size());
        assert!(!OptLevel::Os.is_min_size());
        assert!(OptLevel::Oz.is_min_size());
    }
}
