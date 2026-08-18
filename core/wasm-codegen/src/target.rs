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
//!
//! # Emission Features
//!
//! [`EmitFeatures`] records which post-MVP WebAssembly instruction families code
//! generation may use. It is an independent axis from the mode: the same features
//! apply in `Compile` and `Proof` mode, so the `.v` always describes the same
//! program as the shipped `.wasm`.
//!
//! # Memory Layout
//!
//! [`MemoryLayout`] describes the linear memory a module declares and how much of
//! it the shadow stack occupies. It is the single source of truth for both
//! numbers: the memory section, the `__stack_pointer` initializer, and the
//! per-frame size assertion all read it, so no part of code generation can hold
//! its own idea of where the stack ends. It is defined in
//! `inference-compiler-interface` and re-exported here, because the surfaces
//! that select a layout share it — emission reads exactly the type a manifest or
//! a compiler flag fills in.

/// The linear memory shape, re-exported so [`CodegenOptions`] and every caller
/// naming the field keep one path to it.
///
/// [`MemoryLayout`]'s fields are private and [`MemoryLayout::resolve`] is the
/// only way to name a non-default one, so the constructor's vocabulary comes
/// along: a caller that can set the field must be able to build the value.
pub use inference_compiler_interface::{MemoryLayout, MemoryLayoutError, MemoryLayoutSource};

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
    /// General-purpose WebAssembly target, MVP baseline by default.
    ///
    /// Supports Inference non-deterministic operations via custom 0xfc prefix
    /// instructions. No post-MVP feature is enabled unless the build requests it
    /// through [`EmitFeatures`]; the requestable ones occupy 0xfc sub-opcodes
    /// disjoint from the custom instruction space, so an opt-in never makes a
    /// module ambiguous to decode.
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
/// WASM emission.
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

/// The complete configuration [`crate::codegen`] compiles under: which platform
/// the module targets, which compilation mode drives emission, how the output is
/// optimized, which post-MVP instruction families emission may use, and how the
/// module's linear memory is laid out.
///
/// This is the input mirror of the configuration [`crate::CodegenOutput`]
/// records on the artifact it describes. Bundling the values keeps the
/// `codegen` signature stable as configuration grows: a new knob is a new field
/// here, not a new parameter at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodegenOptions {
    /// The WebAssembly platform the module is compiled for.
    pub target: Target,
    /// Whether emission produces an executable or a proof artifact.
    pub mode: CompilationMode,
    /// The optimization level recorded on the output.
    pub opt_level: OptLevel,
    /// The post-MVP instruction families emission is permitted to use.
    pub features: EmitFeatures,
    /// The linear memory the module declares and the share of it the shadow
    /// stack occupies.
    pub layout: MemoryLayout,
}

/// Implemented by hand rather than derived: the default optimization level is
/// target-derived ([`Target::default_opt_level`]), which a derived `Default`
/// cannot express, and deriving it would silently pin `OptLevel`'s own default
/// instead of the target's.
impl Default for CodegenOptions {
    fn default() -> Self {
        let target = Target::default();
        Self {
            target,
            mode: CompilationMode::default(),
            opt_level: target.default_opt_level(),
            features: EmitFeatures::default(),
            layout: MemoryLayout::default(),
        }
    }
}

/// The post-MVP WebAssembly instruction families code generation is permitted to
/// emit.
///
/// The default — every field `false` — keeps the emitted module inside the
/// WebAssembly 1.0 instruction set, which is what every build produces unless it
/// asks for more. A field is a *permission*, not an instruction: setting
/// `bulk_memory` lets the region fill and copy lowerings use `memory.fill` and
/// `memory.copy` where they otherwise expand to plain loads and stores, and the
/// resulting bytes are those Inference emitted before the WebAssembly 1.0
/// lowering existed.
///
/// Deliberately not named `WasmFeatures`: `inf_wasmparser::WasmFeatures` is the
/// *validation envelope* a module is checked against, which is strictly wider
/// than what code generation knows how to produce, and confusing the two would
/// invite validating against whatever happened to be emitted.
///
/// Independent of [`CompilationMode`]: nothing may gate a field on the mode, or
/// the Rocq translation would describe a different program than the shipped
/// binary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmitFeatures {
    /// Permits `memory.copy` and `memory.fill` for whole-region copies and frame
    /// zero-fills.
    pub bulk_memory: bool,
}

impl EmitFeatures {
    /// The proposal name of the first requested feature `target` does not
    /// accept, or `None` when the whole set is permitted.
    ///
    /// Destructuring `Self` makes a newly added field a compile error here, so a
    /// feature cannot reach code generation without a decision about every
    /// target having been recorded.
    #[must_use]
    pub fn first_rejected_by(self, target: Target) -> Option<&'static str> {
        let Self { bulk_memory } = self;
        if bulk_memory && !target.permits_bulk_memory() {
            return Some("bulk-memory");
        }
        None
    }
}

impl Target {
    /// Whether a module using bulk memory instructions is accepted by this
    /// target's runtime.
    ///
    /// `Soroban` rejects them for now. Whether its validator admits the
    /// bulk-memory opcodes is unverified, and a build-time refusal is a better
    /// failure than a contract that is rejected at deploy time; relax this once
    /// there is evidence.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert!(Target::Wasm32.permits_bulk_memory());
    /// assert!(!Target::Soroban.permits_bulk_memory());
    /// ```
    #[must_use]
    pub fn permits_bulk_memory(self) -> bool {
        matches!(self, Self::Wasm32)
    }

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

    #[test]
    fn emit_features_default_is_wasm_1_0() {
        assert_eq!(EmitFeatures::default(), EmitFeatures { bulk_memory: false });
    }

    #[test]
    fn default_features_are_permitted_by_every_target() {
        for target in [Target::Wasm32, Target::Soroban] {
            assert_eq!(EmitFeatures::default().first_rejected_by(target), None);
        }
    }

    #[test]
    fn wasm32_permits_bulk_memory() {
        assert_eq!(
            EmitFeatures { bulk_memory: true }.first_rejected_by(Target::Wasm32),
            None
        );
    }

    #[test]
    fn soroban_rejects_bulk_memory() {
        assert_eq!(
            EmitFeatures { bulk_memory: true }.first_rejected_by(Target::Soroban),
            Some("bulk-memory")
        );
    }

    /// Every name a user may request must map onto some emission flag, or the
    /// request would validate and then quietly do nothing.
    ///
    /// The exhaustive match enforces that a decision was *made*, not that a
    /// dedicated field exists: a new `WasmFeatureName` fails to compile here until
    /// it has an arm, and that arm may legitimately reuse an existing field when
    /// two proposals gate the same emission. What it cannot do is be omitted. The
    /// inequality against the default is what rules out an arm that decides
    /// nothing.
    #[test]
    fn every_requestable_name_maps_onto_an_emission_flag() {
        use inference_compiler_interface::WasmFeatureName;

        for name in WasmFeatureName::ALL {
            let requested = match name {
                WasmFeatureName::BulkMemory => EmitFeatures { bulk_memory: true },
            };
            assert_ne!(
                requested,
                EmitFeatures::default(),
                "`{}` must set a field",
                name.as_str()
            );
        }
    }
}
