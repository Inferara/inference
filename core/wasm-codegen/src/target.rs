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
//! its own idea of where the stack ends.

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

/// The linear memory a generated module declares, and the share of it the shadow
/// stack occupies.
///
/// Code generation places the shadow stack at the bottom of memory: it spans
/// `[0, stack_size)` and `__stack_pointer` grows downward from `stack_size`
/// toward 0. Whatever lies between `stack_size` and `pages * 64 KiB` is the data
/// region — nothing this compiler emits reads or writes it today, and it is the
/// reason the stack size is an independent value rather than simply the whole
/// memory. It is ordinary addressable memory, not a hole: an access that strays
/// into it succeeds rather than trapping, so a stack larger than the program
/// needs is not free (see `core/wasm-linker`, which today leans on an
/// out-of-region address usually being out of bounds).
///
/// The two numbers form one type because neither is checkable alone: a stack
/// size is only sane relative to the memory it must fit in, a page count is only
/// sane relative to the stack it must hold, and the overflow trap needs the two
/// together to leave headroom below 2^32. [`Self::validate`] is where that joint
/// contract lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLayout {
    /// Linear memory size in 64 KiB pages. Emitted as both the minimum and the
    /// maximum, so the memory is fixed rather than growable.
    pub pages: u32,
    /// Size of the shadow-stack region in bytes, occupying `[0, stack_size)`.
    pub stack_size: u32,
}

/// Implemented by hand rather than derived: a derived `Default` would produce a
/// zero-page, zero-byte memory, which is not a layout any program can run in.
/// These are instead exactly the values every build emitted before the layout
/// became configurable — one page, entirely stack — so a default build's bytes
/// are unchanged.
impl Default for MemoryLayout {
    fn default() -> Self {
        Self {
            pages: 1,
            stack_size: crate::memory::PAGE_SIZE,
        }
    }
}

/// The largest linear memory a 32-bit WebAssembly module may declare: 65536
/// pages of 64 KiB each is the whole 4 GiB address space.
const MAX_PAGES: u32 = 65_536;

/// The 32-bit address space in bytes.
///
/// The stack-overflow trap depends on a wrapped frame pointer landing past the
/// end of memory, so the memory and the stack must fit inside this together —
/// see the headroom invariant in [`MemoryLayout::validate`]. That is a stricter
/// bound than [`MAX_PAGES`] alone, and it is why a module may not declare the
/// whole address space.
const ADDRESS_SPACE: u64 = 1 << 32;

impl MemoryLayout {
    /// Checks that the two sizes describe a linear memory a module can actually
    /// declare and code generation can actually address.
    ///
    /// Destructuring `Self` makes a newly added field a compile error here, so a
    /// dimension of the layout cannot reach code generation without a decision
    /// about its valid range having been recorded.
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant as a message naming the offending
    /// value. Callers surface it verbatim, so it must read as an explanation of
    /// the number the build asked for, not of the check that rejected it.
    pub fn validate(self) -> Result<(), String> {
        let Self { pages, stack_size } = self;
        let page_size = u64::from(crate::memory::PAGE_SIZE);
        let memory_bytes = u64::from(pages) * page_size;

        if pages == 0 {
            return Err(
                "linear memory must be at least one 64 KiB page, but 0 pages were requested"
                    .to_string(),
            );
        }
        if pages > MAX_PAGES {
            return Err(format!(
                "linear memory is limited to {MAX_PAGES} pages (4 GiB) by 32-bit WebAssembly, \
                 but {pages} pages were requested"
            ));
        }
        if stack_size == 0 {
            return Err(
                "the shadow stack must be at least one frame wide, but a size of 0 bytes was \
                 requested"
                    .to_string(),
            );
        }
        if stack_size % crate::memory::FRAME_ALIGNMENT != 0 {
            return Err(format!(
                "the shadow stack size must be a multiple of the {}-byte frame alignment, \
                 because frame sizes are rounded to it and the stack top must land on that \
                 grid, but {stack_size} bytes were requested",
                crate::memory::FRAME_ALIGNMENT
            ));
        }
        if u64::from(stack_size) > memory_bytes {
            return Err(format!(
                "the shadow stack ({stack_size} bytes) does not fit in the linear memory it \
                 lives in ({pages} × 64 KiB = {memory_bytes} bytes)"
            ));
        }
        if stack_size > i32::MAX.cast_unsigned() {
            return Err(format!(
                "the shadow stack size must not exceed {} bytes, the largest value the \
                 `__stack_pointer` initializer can hold as a signed 32-bit constant, but \
                 {stack_size} bytes were requested",
                i32::MAX
            ));
        }
        let span = memory_bytes + u64::from(stack_size);
        if span > ADDRESS_SPACE {
            return Err(format!(
                "the linear memory ({pages} × 64 KiB = {memory_bytes} bytes) and the shadow \
                 stack ({stack_size} bytes) together span {span} bytes, more than the \
                 {ADDRESS_SPACE}-byte 32-bit address space; a stack overflow wraps to an \
                 address at least {ADDRESS_SPACE} minus the stack size, which must stay past \
                 the end of memory for the overflow to trap instead of writing into it"
            ));
        }
        Ok(())
    }

    /// The initial `__stack_pointer` value: one past the last valid stack
    /// address.
    ///
    /// This is a "past-the-end" value (like C++ `vector::end()`). Address
    /// `stack_size` itself is never accessed — a frame prologue subtracts the
    /// frame size before any memory operation, so the first actual access is at
    /// `stack_size - frame_size`.
    ///
    /// The conversion is lossless for every layout [`Self::validate`] accepts,
    /// which is what bounds `stack_size` by [`i32::MAX`].
    #[must_use]
    pub fn stack_pointer_init(self) -> i32 {
        self.stack_size.cast_signed()
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

    #[test]
    fn default_layout_is_one_page_of_stack() {
        assert_eq!(
            MemoryLayout::default(),
            MemoryLayout {
                pages: 1,
                stack_size: 65_536
            }
        );
    }

    #[test]
    fn default_layout_validates() {
        assert_eq!(MemoryLayout::default().validate(), Ok(()));
    }

    #[test]
    fn default_stack_pointer_starts_past_the_last_stack_address() {
        assert_eq!(MemoryLayout::default().stack_pointer_init(), 65_536);
    }

    /// Each invariant is rejected on its own. The expectations pin both the
    /// invariant that fired and the value it names, because several messages
    /// would otherwise be satisfied by the same number: a layout that breaks one
    /// rule must not be reported under another.
    #[test]
    fn validate_rejects_each_broken_invariant() {
        let cases = [
            (
                MemoryLayout {
                    pages: 0,
                    stack_size: 65_536,
                },
                "at least one 64 KiB page",
                "0 pages",
            ),
            (
                MemoryLayout {
                    pages: 65_537,
                    stack_size: 65_536,
                },
                "limited to 65536 pages",
                "65537 pages",
            ),
            (
                MemoryLayout {
                    pages: 1,
                    stack_size: 0,
                },
                "at least one frame wide",
                "0 bytes",
            ),
            (
                MemoryLayout {
                    pages: 1,
                    stack_size: 1_000,
                },
                "multiple of the 16-byte frame alignment",
                "1000 bytes",
            ),
            (
                MemoryLayout {
                    pages: 1,
                    stack_size: 131_072,
                },
                "does not fit in the linear memory",
                "131072 bytes",
            ),
            (
                MemoryLayout {
                    pages: 65_536,
                    stack_size: 2_147_483_664,
                },
                "signed 32-bit constant",
                "2147483664 bytes",
            ),
            // A memory filling the whole address space leaves a wrapped stack
            // pointer nowhere out of bounds to land, so the overflow trap is
            // gone. Every other rule here is satisfied.
            (
                MemoryLayout {
                    pages: 65_536,
                    stack_size: 65_536,
                },
                "32-bit address space",
                "4295032832 bytes",
            ),
        ];
        for (layout, invariant, value) in cases {
            let message = layout
                .validate()
                .expect_err(&format!("{layout:?} must be rejected"));
            assert!(
                message.contains(invariant),
                "{layout:?} was rejected with `{message}`, which is not the `{invariant}` rule"
            );
            assert!(
                message.contains(value),
                "{layout:?} was rejected with `{message}`, which does not name `{value}`"
            );
        }
    }

    /// The largest memory the overflow trap survives is accepted, and so is a
    /// stack strictly smaller than the memory holding it — the shape that leaves
    /// a data region above the stack.
    ///
    /// The first case is one page short of the 32-bit maximum, which is the real
    /// ceiling: the headroom the wrapped stack pointer needs is what costs that
    /// page, not the page count rule.
    #[test]
    fn validate_accepts_the_extremes_of_the_admissible_range() {
        assert_eq!(
            MemoryLayout {
                pages: 65_535,
                stack_size: 16
            }
            .validate(),
            Ok(())
        );
        assert_eq!(
            MemoryLayout {
                pages: 2,
                stack_size: 16
            }
            .validate(),
            Ok(())
        );
    }

    /// The headroom rule is exactly `memory_bytes + stack_size <= 2^32`, so a
    /// layout one byte-grid step either side of the boundary must land on
    /// opposite verdicts. Without this pair the rule could be off by a whole
    /// page and every other test would still pass.
    #[test]
    fn the_address_space_headroom_boundary_is_exact() {
        let fits = MemoryLayout {
            pages: 65_535,
            stack_size: 65_536,
        };
        assert_eq!(
            u64::from(fits.pages) * 65_536 + u64::from(fits.stack_size),
            1 << 32,
            "this case is meant to sit exactly on the boundary"
        );
        assert_eq!(fits.validate(), Ok(()));

        let overflows = MemoryLayout {
            stack_size: fits.stack_size + 16,
            ..fits
        };
        assert!(
            overflows.validate().is_err(),
            "one frame past the boundary must be rejected"
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
