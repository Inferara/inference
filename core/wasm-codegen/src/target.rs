//! Compilation target and mode definitions for the Inference compiler.
//!
//! This module defines the target platform, compilation mode, and optimization level
//! types used throughout the code generation pipeline. These types control how WASM
//! bytecode is generated.
//!
//! # Target
//!
//! The [`Target`] enum specifies the WebAssembly target platform:
//! - [`Target::Wasm32`] -- General-purpose WASM with Inference non-deterministic
//!   instruction support. Used for both verification (`proof` mode) and general
//!   execution (`compile` mode).
//! - [`Target::Soroban`] -- Stellar Soroban smart contract target for standard code
//!   without non-deterministic instructions.
//!
//! # Compilation Mode
//!
//! The [`CompilationMode`] enum controls spec-node handling:
//! - [`CompilationMode::Compile`] -- Produces production binaries. Spec nodes are
//!   stripped from codegen since they have no runtime meaning.
//! - [`CompilationMode::Proof`] -- Produces WASM for formal verification. Spec functions
//!   (containing non-deterministic operations) are compiled to preserve 1:1 structural
//!   correspondence for Rocq formalization. Execution functions use the target's release
//!   optimization.
//!
//! # Optimization Level
//!
//! The [`OptLevel`] enum represents optimization levels. These are preserved for future
//! integration with wasm-opt or similar post-processing tools.

/// Compilation target for code generation.
///
/// Both targets produce WebAssembly modules but differ in which WASM features and
/// non-deterministic instructions are permitted.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::Target;
///
/// let target = Target::default();
/// assert_eq!(target, Target::Wasm32);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    /// General-purpose WebAssembly target with strict MVP baseline.
    ///
    /// Supports Inference non-deterministic operations via custom 0xfc prefix
    /// instructions. No post-MVP features are enabled, ensuring compatibility with
    /// the custom instruction space.
    ///
    /// Used in both `compile` and `proof` modes.
    #[default]
    Wasm32,

    /// Stellar Soroban smart contract target.
    ///
    /// Produces size-optimized binaries to fit within the 64 KB contract size limit.
    ///
    /// Only supports `compile` mode -- `proof` mode requires custom intrinsics that
    /// are incompatible with the Soroban VM.
    Soroban,
}

/// Compilation mode controlling spec-node handling.
///
/// The mode is orthogonal to the target: `compile` mode works with any target,
/// while `proof` mode requires the `Wasm32` target (custom non-deterministic
/// instructions need the Wasm32 target).
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
    /// runtime meaning. The target's default optimization level applies.
    #[default]
    Compile,

    /// Produces WASM for formal verification via Rocq translation.
    ///
    /// All code (including spec functions with non-deterministic instructions) is
    /// emitted into the WASM module. Spec functions preserve 1:1 structural
    /// correspondence with the source code for Rocq readability. Execution functions
    /// are compiled at the target's default release optimization so that Rocq proofs
    /// cover the actual deployed code (Decision #32).
    ///
    /// The target is always `Wasm32` -- custom non-deterministic instructions require
    /// the Wasm32 target.
    Proof,
}

/// Optimization level for compilation.
///
/// These levels are preserved for future integration with wasm-opt or similar
/// post-processing tools. Currently, no optimization pass is applied during
/// direct WASM emission.
///
/// # Examples
///
/// ```
/// use inference_wasm_codegen::OptLevel;
///
/// let level = OptLevel::Oz;
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
    /// Optimize for size.
    Os,
    /// Aggressively optimize for size.
    Oz,
}

impl OptLevel {
    /// Whether to optimize for smaller code size.
    ///
    /// When true, the compiler should prefer smaller code size over execution speed.
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

    /// Whether to aggressively minimize code size.
    ///
    /// When true, the compiler should aggressively minimize code size, even at the
    /// expense of execution speed. This is only set for the `Oz` level.
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
    /// Returns whether this target supports proof mode.
    ///
    /// Only `Wasm32` supports proof mode because it uses custom 0xfc non-deterministic
    /// instructions for formal verification. Other targets (e.g., `Soroban`) cannot
    /// process these custom instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert!(Target::Wasm32.supports_proof_mode());
    /// assert!(!Target::Soroban.supports_proof_mode());
    /// ```
    #[must_use]
    pub fn supports_proof_mode(&self) -> bool {
        matches!(self, Self::Wasm32)
    }

    /// Returns the default optimization level for this target.
    ///
    /// | Target  | `OptLevel` |
    /// |---------|----------|
    /// | Wasm32  | O3       |
    /// | Soroban | Oz       |
    ///
    /// The optimization level is target-specific and mode-independent. In `proof`
    /// mode, spec functions are emitted without optimization to preserve structural
    /// correspondence. Execution functions are compiled at the target's release
    /// optimization so that Rocq proofs cover the actual deployed code (Decision #32).
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::{Target, OptLevel};
    ///
    /// assert_eq!(Target::Wasm32.default_opt_level(), OptLevel::O3);
    /// assert_eq!(Target::Soroban.default_opt_level(), OptLevel::Oz);
    /// ```
    #[must_use]
    pub fn default_opt_level(&self) -> OptLevel {
        match self {
            Self::Wasm32 => OptLevel::O3,
            Self::Soroban => OptLevel::Oz,
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
    fn wasm32_default_opt_level_is_o3() {
        assert_eq!(Target::Wasm32.default_opt_level(), OptLevel::O3);
    }

    #[test]
    fn soroban_default_opt_level_is_oz() {
        assert_eq!(Target::Soroban.default_opt_level(), OptLevel::Oz);
    }

    #[test]
    fn wasm32_supports_proof_mode() {
        assert!(Target::Wasm32.supports_proof_mode());
    }

    #[test]
    fn soroban_does_not_support_proof_mode() {
        assert!(!Target::Soroban.supports_proof_mode());
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
