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
//! - [`Target::Stellar`] -- Stellar smart contract target for standard code
//!   without non-deterministic instructions, whose exported functions are held
//!   to the scalar set the contract calling convention can encode.
//! - [`Target::SpaceWasm`] -- The `SpaceWasm` flight interpreter, for standard
//!   code without non-deterministic instructions. It narrows what a build may
//!   request and changes nothing about what is emitted.
//!
//! # Compilation Mode
//!
//! The [`CompilationMode`] enum controls spec-node handling:
//! - [`CompilationMode::Compile`] -- Produces production binaries. Spec nodes are
//!   stripped from codegen since they have no runtime meaning.
//! - [`CompilationMode::Proof`] -- Produces WASM for formal verification. Spec functions
//!   (containing non-deterministic operations) are compiled to preserve 1:1 structural
//!   correspondence for Rocq formalization. Execution functions are emitted exactly as
//!   `compile` mode would emit them: codegen applies no optimization pass in either mode
//!   (see [`OptLevel`]), so an execution function's bytes are identical across modes and
//!   Rocq proofs cover the artifact that actually ships.
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
/// Every target produces a WebAssembly module; what differs is which WASM
/// features, compilation modes and non-deterministic instructions a build for it
/// may request.
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

    /// Stellar smart contract target.
    ///
    /// The target narrows what a build may *request*; it does not change what
    /// code generation produces. For any configuration both targets accept,
    /// emission is byte-for-byte what [`Target::Wasm32`] emits -- nothing on the
    /// emission path reads a `Target`. What differs is the envelope: `proof` mode
    /// is refused because its custom 0xfc intrinsics are not decodable by the
    /// Stellar VM, a bulk-memory request is refused (see
    /// [`Target::permits_bulk_memory`]), an exported function outside the scalar
    /// set the contract calling convention can encode is refused, and
    /// [`OptLevel::Oz`] is this target's
    /// [`default_opt_level`](Target::default_opt_level), for a post-build tool
    /// to act on. The level actually recorded on the output is whatever the
    /// caller passes: the `Debug` build profile resolves this target to
    /// [`OptLevel::O0`], and only `Release` takes the target default. No
    /// optimization pass runs during emission (see [`OptLevel`]) and no contract
    /// size limit is checked anywhere in the compiler.
    Stellar,

    /// The `SpaceWasm` flight interpreter.
    ///
    /// `SpaceWasm` is NASA JPL's `no_std` WebAssembly 1.0 interpreter, written for
    /// flight software: it decodes a module into its own IR on a fixed
    /// allocation and executes it with no host operating system underneath. Like
    /// [`Target::Stellar`] this target narrows what a build may *request* and
    /// changes nothing about what is produced -- for any configuration both
    /// accept, emission is byte-for-byte what [`Target::Wasm32`] emits, because
    /// nothing on the emission path reads a `Target`. Unlike Stellar there is no
    /// rewrite afterwards either, so the deployed module and the module a proof
    /// is written about are the same file, not one derived from the other.
    ///
    /// What the envelope narrows:
    ///
    /// - `proof` mode is refused: the custom 0xfc intrinsics it emits are not in
    ///   the instruction set the interpreter decodes.
    /// - A non-deterministic construct in an executable function is refused for
    ///   the same reason (see [`Target::supports_non_det_functions`]). Analysis
    ///   rules A042 and A006 are what make that refusal total -- A042 the
    ///   non-deterministic blocks, A006 the bare `@` A042 leaves to it; the
    ///   code-generation gate behind the predicate is the coarser backstop for a
    ///   caller that never ran analysis.
    /// - A bulk-memory request is refused: the decoder has not implemented the
    ///   proposal (nasa/spacewasm#54). The sign-extension and
    ///   saturating-truncation proposals are open upstream beside it
    ///   (nasa/spacewasm#55 and #56) and cost this target nothing, because code
    ///   generation emits no instruction from either family at any target.
    /// - [`OptLevel::Os`] is this target's
    ///   [`default_opt_level`](Target::default_opt_level), size being the scarce
    ///   resource on a flight computer. As everywhere, the level is recorded for
    ///   a post-build tool and applied by nothing during emission.
    ///
    /// The interpreter also enforces decode-time maxima -- parameter and local
    /// words per function, name and custom-section byte caps, and
    /// embedder-configured control-frame and operand-stack depths -- which
    /// nothing in this crate checks today. A module this target accepts is
    /// therefore inside the *instruction set* the interpreter decodes, which is
    /// not yet the same statement as one it will load.
    SpaceWasm,
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
    /// Produces production binaries.
    ///
    /// Non-deterministic `spec` nodes are stripped from codegen since they have no
    /// runtime meaning. Codegen applies no optimization pass; the target's default
    /// [`OptLevel`] is recorded on the output for a downstream tool (e.g. a
    /// `[build.wasm-opt]` post-build step) to act on, not applied here.
    #[default]
    Compile,

    /// Produces WASM for formal verification via Rocq translation.
    ///
    /// All code (including spec functions with non-deterministic instructions) is
    /// emitted into the WASM module. Codegen applies no optimization pass in this
    /// mode either, so an execution function's bytes are byte-for-byte identical to
    /// what `Compile` mode would emit for the same source -- Rocq proofs therefore
    /// cover the artifact that actually ships, with no separate "unoptimized proof
    /// build" to fall out of sync with it. Spec functions carry no counterpart in
    /// `Compile` mode at all: they are structurally lowered 1:1 from the source for
    /// Rocq readability, which is a property of the lowering, not of an optimizer
    /// being withheld.
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
    /// Every target code generation can emit for, in canonical order.
    ///
    /// This is the emission side of the axis and is deliberately allowed to be
    /// wider than `inference_compiler_interface::TargetName`: a target reaches
    /// this enum when the emitter knows what to do with it, and the shared
    /// vocabulary when a user may ask for it, and those are two different dates.
    /// The cross-check that every requestable name lands on a target here lives
    /// in this module's tests.
    pub const ALL: [Self; 3] = [Self::Wasm32, Self::Stellar, Self::SpaceWasm];

    /// This target's canonical name: for a target a user may ask for, the
    /// lowercase spelling `inference_compiler_interface::TargetName` uses for
    /// it, so the two enums can be checked against each other rather than
    /// trusted to agree. A target this enum carries ahead of that vocabulary
    /// -- see [`Target::ALL`] on why it may -- names itself here in the spelling
    /// the vocabulary will use for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wasm32 => "wasm32",
            Self::Stellar => "stellar",
            Self::SpaceWasm => "spacewasm",
        }
    }

    /// Whether a module using bulk memory instructions is accepted by this
    /// target's runtime.
    ///
    /// Every target but the default one answers `false`, pinning its output to
    /// the WebAssembly 1.0 instruction set -- but *why* differs per target, and
    /// each variant's own documentation is where its reason lives. One says the
    /// runtime would accept the instructions and the pin is a deliberate
    /// conservatism; the other says the decoder has not implemented them. Read
    /// as a general claim about targets, either reason would be wrong about the
    /// other.
    ///
    /// What the two share is the consequence, and it is why the answer is `false`
    /// in both cases rather than merely defensible: no lowering any target can
    /// reach needs the instructions (the region fill and copy lowerings have a
    /// load/store form), WebAssembly 1.0 is what every deployment path accepts,
    /// and a build-time refusal is a better failure than a module rejected on
    /// arrival.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert!(Target::Wasm32.permits_bulk_memory());
    /// assert!(!Target::Stellar.permits_bulk_memory());
    /// assert!(!Target::SpaceWasm.permits_bulk_memory());
    /// ```
    #[must_use]
    pub fn permits_bulk_memory(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Returns whether this target supports proof mode.
    ///
    /// Only `Wasm32` supports proof mode, because proof mode's output is built
    /// out of the custom 0xfc non-deterministic instructions and no runtime but a
    /// general-purpose WebAssembly embedder under our own tooling decodes them.
    /// Every other target refuses the mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert!(Target::Wasm32.supports_proof_mode());
    /// assert!(!Target::Stellar.supports_proof_mode());
    /// assert!(!Target::SpaceWasm.supports_proof_mode());
    /// ```
    #[must_use]
    pub fn supports_proof_mode(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Returns whether this target's runtime can execute a function containing a
    /// non-deterministic construct.
    ///
    /// Uzumaki (`@`) and the `forall`/`exists`/`assume`/`unique` blocks lower to
    /// the same custom 0xfc instructions proof mode is made of, so a target
    /// whose runtime is somebody else's answers `false`, and analysis rules A042
    /// and A006 refuse a build carrying one in a function that ships -- A042 the
    /// blocks, A006 the bare `@` A042 leaves to it. Code generation re-asks the
    /// question for a caller that skipped analysis, but with a coarser walk than
    /// theirs -- see [`crate::codegen`].
    ///
    /// Separate from [`Self::supports_proof_mode`] because the two questions come
    /// apart: that one asks whether a build may request the mode whose entire
    /// output is such instructions, this one whether an ordinary `compile`-mode
    /// program may contain any at all. `compile` mode strips `spec` bodies, so
    /// the constructs a specification is written in cost a refusal only where
    /// they sit in executable code -- which is what makes "move it into a
    /// `spec` block" a real remedy rather than a deletion.
    ///
    /// # Examples
    ///
    /// ```
    /// use inference_wasm_codegen::Target;
    ///
    /// assert!(Target::Wasm32.supports_non_det_functions());
    /// assert!(!Target::Stellar.supports_non_det_functions());
    /// assert!(!Target::SpaceWasm.supports_non_det_functions());
    /// ```
    #[must_use]
    pub fn supports_non_det_functions(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Returns the default optimization level for this target.
    ///
    /// | Target        | `OptLevel` |
    /// |---------------|------------|
    /// | `Wasm32`      | `O3`       |
    /// | `Stellar`     | `Oz`       |
    /// | `SpaceWasm`   | `Os`       |
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
    /// assert_eq!(Target::Stellar.default_opt_level(), OptLevel::Oz);
    /// assert_eq!(Target::SpaceWasm.default_opt_level(), OptLevel::Os);
    /// ```
    #[must_use]
    pub fn default_opt_level(self) -> OptLevel {
        match self {
            Self::Wasm32 => OptLevel::O3,
            Self::Stellar => OptLevel::Oz,
            Self::SpaceWasm => OptLevel::Os,
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
    fn stellar_default_opt_level_is_oz() {
        assert_eq!(Target::Stellar.default_opt_level(), OptLevel::Oz);
    }

    #[test]
    fn spacewasm_default_opt_level_is_os() {
        assert_eq!(Target::SpaceWasm.default_opt_level(), OptLevel::Os);
    }

    #[test]
    fn wasm32_supports_proof_mode() {
        assert!(Target::Wasm32.supports_proof_mode());
    }

    #[test]
    fn stellar_does_not_support_proof_mode() {
        assert!(!Target::Stellar.supports_proof_mode());
    }

    #[test]
    fn spacewasm_does_not_support_proof_mode() {
        assert!(!Target::SpaceWasm.supports_proof_mode());
    }

    /// The three predicates answer the same way for every target but the
    /// default, and the exhaustive matches behind them are what force a fourth
    /// target to state three decisions rather than inherit them. Iterating
    /// `Target::ALL` is what makes this a statement about the set rather than
    /// about the two variants somebody remembered to name.
    ///
    /// This fails if a target is added that permits one of the three without
    /// being the default, which is the moment the envelope stops being "Wasm32
    /// or a narrowing of it" and the callers reading these predicates need
    /// re-examining.
    #[test]
    fn only_the_default_target_admits_the_compiler_s_own_extensions() {
        for target in Target::ALL {
            let is_default = target == Target::default();
            assert_eq!(
                target.supports_proof_mode(),
                is_default,
                "`{}` disagrees with the default about proof mode",
                target.as_str()
            );
            assert_eq!(
                target.supports_non_det_functions(),
                is_default,
                "`{}` disagrees with the default about non-deterministic functions",
                target.as_str()
            );
            assert_eq!(
                target.permits_bulk_memory(),
                is_default,
                "`{}` disagrees with the default about bulk memory",
                target.as_str()
            );
        }
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
        for target in Target::ALL {
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
    fn stellar_rejects_bulk_memory() {
        assert_eq!(
            EmitFeatures { bulk_memory: true }.first_rejected_by(Target::Stellar),
            Some("bulk-memory")
        );
    }

    #[test]
    fn spacewasm_rejects_bulk_memory() {
        assert_eq!(
            EmitFeatures { bulk_memory: true }.first_rejected_by(Target::SpaceWasm),
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

    /// Every target a user may request must map onto an emission target that
    /// agrees with it on the name and on what it permits, or the two enums have
    /// drifted and a request resolves to something other than what it spells.
    ///
    /// The exhaustive match is what enforces that the mapping *exists*: a new
    /// `TargetName` fails to compile here until an emission target is decided for
    /// it. The name equality is what catches the other drift — the same runtime
    /// entering both enums under two spellings, which compiles fine and sends
    /// every diagnostic and every manifest key to the wrong string.
    ///
    /// The predicate equalities catch the drift that costs a user a wrong
    /// artifact rather than a wrong word. `TargetName` carries its own copies of
    /// them so a front end holding only a name can refuse a combination before
    /// spawning a compiler; if a copy ever said yes where emission says no, that
    /// front end would forward a build this crate then refuses, and if it said
    /// no where emission says yes it would refuse a build that works. Neither
    /// enum is the authority on its own — they must simply agree.
    #[test]
    fn every_requestable_target_maps_onto_an_emission_target() {
        use inference_compiler_interface::TargetName;

        for name in TargetName::ALL {
            let emitted = match name {
                TargetName::Wasm32 => Target::Wasm32,
                TargetName::Stellar => Target::Stellar,
            };
            assert_eq!(
                name.as_str(),
                emitted.as_str(),
                "the requestable name and the emission target disagree on the spelling"
            );
            assert_eq!(
                name.supports_proof_mode(),
                emitted.supports_proof_mode(),
                "`{}` disagrees with its emission target about proof mode",
                name.as_str()
            );
            assert_eq!(
                name.permits_bulk_memory(),
                emitted.permits_bulk_memory(),
                "`{}` disagrees with its emission target about bulk memory",
                name.as_str()
            );
        }
    }

    /// `Target::ALL` is what every "for each target" check iterates, so a variant
    /// missing from it is a target nothing checks. The exhaustive match makes a
    /// new variant a compile error here, and the count is the reminder to add it.
    #[test]
    fn all_lists_every_emission_target_once() {
        for target in Target::ALL {
            match target {
                Target::Wasm32 | Target::Stellar | Target::SpaceWasm => {}
            }
        }
        assert_eq!(
            Target::ALL.len(),
            3,
            "a new target must be added to `Target::ALL`"
        );

        let mut names: Vec<&str> = Target::ALL.iter().map(|t| t.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            Target::ALL.len(),
            "two targets share a name: {names:?}"
        );
    }

    #[test]
    fn each_target_is_named_in_lowercase_kebab_case() {
        for target in Target::ALL {
            let name = target.as_str();
            assert!(
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "`{name}` is not a manifest-spellable target name"
            );
        }
    }
}
