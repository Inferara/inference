//! The vocabulary `infs` and `infc` share: the interface version they handshake
//! on, and the build settings a user selects on either surface.
//!
//! A setting belongs here once more than one front end can express it, so that
//! the manifest and the command line accept the same values and reject the rest
//! with the same words. What each setting then *means* to a later phase stays
//! with that phase.
//!
//! # Compiler ABI version
//!
//! The ABI (application binary interface) here means the set of CLI flags,
//! stdin/stdout contract, and exit codes that `infs` relies on when
//! invoking `infc` as a subprocess. Bump the major on any breaking change;
//! bump the minor on additive, backward-compatible changes.
//!
//! Single source of truth: [`COMPILER_ABI_MAJOR`] and [`COMPILER_ABI_MINOR`].
//! Callers that need a `<major>.<minor>` string format it with those
//! constants directly — keeping the numeric and string forms from drifting.
//!
//! # WebAssembly feature vocabulary
//!
//! [`WasmFeatureName`] is the set of post-MVP WebAssembly proposals a user may
//! *request*, and the diagnostics that reject a bad request live here too, so
//! `infs` (reading `Inference.toml`) and `infc` (reading `--wasm-features`)
//! reject the same spellings with the same words. It is deliberately not derived
//! from what any downstream consumer *tolerates* — the linker's validation
//! envelope and Binaryen's flag set are both strictly wider than what code
//! generation knows how to emit.
//!
//! # Memory layout vocabulary
//!
//! [`MemoryLayout`] is the linear memory a build asks for — the page count and
//! the shadow stack's share of it — together with the invariants that make those
//! two numbers a memory a module can actually declare. It lives here for the
//! same reason the feature vocabulary does: the surfaces that select a layout
//! and the code generation that emits one must agree on which layouts exist, and
//! a rejection has to read the same whether the numbers came from a manifest or
//! a command line.
//!
//! [`MemoryLayout::resolve`] is the only way to name a layout other than the
//! default, so the invariants hold by construction rather than by a check every
//! consumer has to remember to run.

pub mod errors;

pub use crate::errors::{MemoryLayoutError, WasmFeatureError};

/// Breaking ABI changes: incompatible CLI flag removal/rename, stdout contract
/// changes, exit-code semantics changes.
pub const COMPILER_ABI_MAJOR: u32 = 1;

/// Additive changes: new flags, new stdout fields, new exit codes.
///
/// Minor 1 adds the additive `--out-dir <path>` flag to `infc`, letting callers
/// redirect the `out/` artifact directory. It is backward compatible: omitting
/// the flag preserves the prior `out/`-relative-to-CWD behavior, so an `infs`
/// built against minor 0 still pairs with a minor-1 `infc` and vice versa
/// (the older side simply never sends/sees the flag).
///
/// Minor 2 adds the additive `--wasm-features <list>` flag to `infc`, opting the
/// emitted module into the post-MVP instruction families named in
/// [`WasmFeatureName`]. It is backward compatible in the same sense: omitting
/// the flag yields the pure WebAssembly 1.0 output that minor 1 always
/// produced. The pairing is not symmetric, though, and callers must gate on it:
/// a minor-1 `infc` has no way to report that it ignored a requested feature, so
/// an `infs` that forwards the flag must first confirm the minor it is talking
/// to, and refuse rather than silently ship a module at the wrong instruction
/// level.
///
/// Minor 3 adds the additive `--memory-pages <N>` and `--stack-size <BYTES>`
/// flags to `infc`, selecting the linear memory the emitted module declares and
/// the share of it the shadow stack occupies. It is backward compatible in the
/// same sense: omitting both yields the single all-stack page every earlier
/// minor emitted, so a minor-2 `infs` still pairs with a minor-3 `infc`. The
/// reverse pairing is the one callers must gate on. A minor-2 `infc` cannot
/// honor a layout request at all, so an `infs` that forwards must first confirm
/// the minor it is talking to, and refuse rather than ship a module whose memory
/// is not the one the manifest asked for. What an ungated forward produces is
/// not silence but an argument-parser error naming a flag the user never typed;
/// the gate is what turns that into a message naming the `[memory]` table to
/// remove or the toolchain to update.
pub const COMPILER_ABI_MINOR: u32 = 3;

/// A post-MVP WebAssembly proposal that a project may opt into.
///
/// Names are **proposal**-grained, never instruction-grained: every consumer of
/// the choice (a Binaryen `--enable-*` flag, a `wasmparser` feature bit, the
/// linker's validation envelope) is proposal-grained, and which instruction of a
/// family appears at which site is a code-generation decision no user can
/// usefully steer.
///
/// Adding a variant carries a documentation obligation — the per-variant doc
/// comment must record the proposal, the Binaryen flag, the `wasmparser` bit,
/// the opcode range together with an audit against Inference's own `0xFC`
/// sub-opcodes (`0x31`, `0x32`, `0x3A`–`0x3D`), and whether the Rocq translator
/// handles the new instructions. A name must not enter this vocabulary before
/// code generation can act on it: the `infc` mapping from name to emission flags
/// is an exhaustive match, so a variant with no wired effect is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WasmFeatureName {
    /// Bulk memory operations.
    ///
    /// - **Proposal**: bulk memory operations, folded into the WebAssembly 2.0
    ///   baseline. Inference's default output predates it and stays at 1.0.
    /// - **Binaryen flag**: `--enable-bulk-memory`.
    /// - **`wasmparser` bit**: `WasmFeatures::BULK_MEMORY`.
    /// - **Opcodes**: the `0xFC` prefix, sub-opcodes `0x08`–`0x0E`
    ///   (`memory.init`, `data.drop`, `memory.copy`, `memory.fill`,
    ///   `table.init`, `elem.drop`, `table.copy`). Code generation emits only
    ///   `memory.copy` and `memory.fill` of that set. **`0xFC`-squat audit**:
    ///   Inference's non-deterministic instructions occupy `0xFC 0x31`,
    ///   `0xFC 0x32` and `0xFC 0x3A`–`0xFC 0x3D`, all above this range, so the
    ///   two sets are disjoint and a decoder that accepts both stays
    ///   unambiguous.
    /// - **Rocq translation**: supported. `memory.copy` and `memory.fill`
    ///   translate to the `BI_memory_copy` and `BI_memory_fill` instruction
    ///   constructors.
    BulkMemory,
}

impl WasmFeatureName {
    /// Every requestable feature, in canonical order. The rendered supported-set
    /// listing in diagnostics comes from here, so a new variant surfaces in
    /// every "unknown feature" message with no further edit.
    pub const ALL: [Self; 1] = [Self::BulkMemory];

    /// The proposal name as it is written in `Inference.toml` and on the
    /// `--wasm-features` command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BulkMemory => "bulk-memory",
        }
    }

    /// The feature written exactly as `name`, or `None`.
    ///
    /// The inverse of [`Self::as_str`]: matching is exact and case-sensitive,
    /// and no whitespace is trimmed. Manifest values are conventionally
    /// lowercase, and accepting near-misses would make a typo silently change
    /// the instruction level of a shipped artifact.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == name)
    }
}

/// Features that every Inference module already relies on, and which therefore
/// cannot be requested.
///
/// `mutable-globals` is here because the shadow stack's `__stack_pointer` is a
/// mutable global in every module that allocates a frame; the feature is part of
/// the baseline rather than an opt-in.
pub const INHERENT_WASM_FEATURES: &[&str] = &["mutable-globals"];

/// Instruction names mapped to the proposal that introduced them, for the
/// did-you-mean when a user writes an instruction where a proposal name belongs.
///
/// Seeded only with the instructions of proposals in [`WasmFeatureName`], so an
/// entry can never suggest a name the vocabulary does not accept. Only the
/// memory instructions of the bulk-memory family appear: Inference emits no
/// tables, so a user reaching for `table.copy` is not the mistake this table
/// exists to catch.
pub const INSTRUCTION_TO_PROPOSAL: &[(&str, WasmFeatureName)] = &[
    ("memory.init", WasmFeatureName::BulkMemory),
    ("memory.copy", WasmFeatureName::BulkMemory),
    ("memory.fill", WasmFeatureName::BulkMemory),
    ("data.drop", WasmFeatureName::BulkMemory),
];

/// The proposal that introduced the instruction named `instruction`, or `None`
/// when the name is not a known instruction of a supported proposal.
#[must_use]
pub fn proposal_for_instruction(instruction: &str) -> Option<WasmFeatureName> {
    INSTRUCTION_TO_PROPOSAL
        .iter()
        .find(|(name, _)| *name == instruction)
        .map(|(_, proposal)| *proposal)
}

/// Which surface a feature request was written on, so a diagnostic can name the
/// exact thing the user has to edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmFeatureSource {
    /// The `wasm-features` array of a project's `Inference.toml` `[build]`
    /// table.
    Manifest,
    /// The `--wasm-features` flag on an `infc` command line.
    Flag,
}

impl WasmFeatureSource {
    /// The backtick-quoted surface name a message points the user at.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manifest => "`[build] wasm-features`",
            Self::Flag => "`--wasm-features`",
        }
    }
}

/// The supported set as a diagnostic renders it: backtick-quoted names, comma
/// separated, in canonical order.
#[must_use]
pub fn supported_features_listing() -> String {
    WasmFeatureName::ALL
        .iter()
        .map(|f| format!("`{}`", f.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The canonical rendering of a resolved feature set: names sorted, comma
/// separated, no spaces.
///
/// This is both the value `infs` forwards as `--wasm-features` and the set it
/// echoes at build time, so the sort keeps a build line and a forwarded flag
/// independent of the order the entries happened to be written in.
#[must_use]
pub fn render_feature_list(features: &[WasmFeatureName]) -> String {
    let mut names: Vec<&str> = features.iter().map(|f| f.as_str()).collect();
    names.sort_unstable();
    names.join(",")
}

/// Rejects a name that is not in the vocabulary, listing what is.
///
/// The wording lives on [`WasmFeatureError::UnknownFeature`]; this renders it for
/// a caller that wants the string rather than the typed error.
#[must_use]
pub fn unknown_feature_message(entry: &str, surface: WasmFeatureSource) -> String {
    WasmFeatureError::UnknownFeature {
        entry: entry.to_string(),
        surface,
    }
    .to_string()
}

/// Rejects an instruction name written where a proposal name belongs, naming the
/// proposal to write instead. Renders [`WasmFeatureError::InstructionName`].
#[must_use]
pub fn instruction_name_message(
    entry: &str,
    proposal: WasmFeatureName,
    surface: WasmFeatureSource,
) -> String {
    WasmFeatureError::InstructionName {
        entry: entry.to_string(),
        proposal,
        surface,
    }
    .to_string()
}

/// Rejects a feature that is always on, explaining why it cannot be requested.
/// Renders [`WasmFeatureError::InherentFeature`].
#[must_use]
pub fn inherent_feature_message(entry: &str, surface: WasmFeatureSource) -> String {
    WasmFeatureError::InherentFeature {
        entry: entry.to_string(),
        surface,
    }
    .to_string()
}

/// Rejects a feature listed more than once. Renders
/// [`WasmFeatureError::DuplicateFeature`].
#[must_use]
pub fn duplicate_feature_message(entry: &str, surface: WasmFeatureSource) -> String {
    WasmFeatureError::DuplicateFeature {
        entry: entry.to_string(),
        surface,
    }
    .to_string()
}

/// Resolves the raw entries of a feature request into the vocabulary, or returns
/// the one diagnostic that rejects it.
///
/// This is the whole validation both front ends run — not just the wording — so
/// the order the failure families are checked in is decided once. Each entry is
/// classified before the next is looked at, and the most specific family wins:
/// an always-on feature and an instruction name each have a bespoke message that
/// the generic "unknown feature" fallback would bury. A duplicate is only
/// reported for an entry that resolved, so a repeated typo is reported as the
/// typo it is.
///
/// The returned features keep their input order; [`render_feature_list`] is the
/// canonical rendering.
///
/// # Errors
///
/// Returns the [`WasmFeatureError`] for the first entry that is not a valid,
/// not-yet-seen feature name.
pub fn resolve_wasm_features(
    entries: &[String],
    surface: WasmFeatureSource,
) -> Result<Vec<WasmFeatureName>, WasmFeatureError> {
    let mut resolved: Vec<WasmFeatureName> = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry.as_str();
        if INHERENT_WASM_FEATURES.contains(&entry) {
            return Err(WasmFeatureError::InherentFeature {
                entry: entry.to_string(),
                surface,
            });
        }
        if let Some(proposal) = proposal_for_instruction(entry) {
            return Err(WasmFeatureError::InstructionName {
                entry: entry.to_string(),
                proposal,
                surface,
            });
        }
        let Some(feature) = WasmFeatureName::from_name(entry) else {
            return Err(WasmFeatureError::UnknownFeature {
                entry: entry.to_string(),
                surface,
            });
        };
        if resolved.contains(&feature) {
            return Err(WasmFeatureError::DuplicateFeature {
                entry: entry.to_string(),
                surface,
            });
        }
        resolved.push(feature);
    }
    Ok(resolved)
}

/// One WASM memory page in bytes.
///
/// The unit [`MemoryLayout::pages()`] counts in, and the size of the default
/// layout's single page.
pub const PAGE_SIZE: u32 = 65536;

/// Stack frame alignment in bytes (matches LLVM/Rust WASM convention).
///
/// A shadow stack must be a whole number of frames wide, so a layout is checked
/// against the same grid code generation rounds every frame to.
pub const FRAME_ALIGNMENT: u32 = 16;

/// Which surface a layout request was written on, so a diagnostic can name the
/// exact thing the user has to edit.
///
/// Both keys of a surface are named together because [`MemoryLayout::resolve`]
/// checks the two numbers jointly: several invariants — a stack that outgrows
/// its memory, a memory that leaves the overflow trap no room — are properties
/// of the pair, and attributing those to one key would name the wrong one half
/// the time. The `reason` on [`MemoryLayoutError`] identifies the offending
/// value; this identifies where it was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayoutSource {
    /// The `pages` / `stack-size` keys of a project's `Inference.toml`
    /// `[memory]` table.
    Manifest,
    /// The `--memory-pages` / `--stack-size` flags on an `infc` command line.
    Flag,
}

impl MemoryLayoutSource {
    /// The backtick-quoted keys a message points the user at.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manifest => "`[memory] pages` / `[memory] stack-size`",
            Self::Flag => "`--memory-pages` / `--stack-size`",
        }
    }
}

/// The linear memory a generated module declares, and the share of it the shadow
/// stack occupies.
///
/// This is shared vocabulary rather than a code-generation detail: a project's
/// manifest, the compiler flags that override it, and the emitter that turns the
/// two numbers into a memory section and a `__stack_pointer` initializer all read
/// this one type. A layout a build accepts is therefore exactly a layout that can
/// be emitted.
///
/// It is deliberately not mirrored by a code-generation twin the way
/// [`WasmFeatureName`] is by `EmitFeatures`. That mirror earns its place because
/// a requested proposal and an emission permission are genuinely different
/// things: the set a user may ask for is not the set an emitter knows how to
/// produce. A layout has no such second reading — the pages and stack bytes a
/// user writes are the pages and stack bytes emitted — so a mirror would buy
/// nothing but a second place for the invariants below to drift apart.
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
/// together to leave headroom below 2^32. [`Self::resolve`] is where that joint
/// contract lives, and it is the only way to name a layout other than
/// [`Self::default`]. The fields are private so that holding a value of this
/// type *is* the guarantee that the contract holds — a consumer reads the two
/// numbers through [`Self::pages()`] and [`Self::stack_size()`] without owing
/// anyone a validation step, and no caller can assemble a memory the emitter
/// would have to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLayout {
    /// Linear memory size in 64 KiB pages. Emitted as both the minimum and the
    /// maximum, so the memory is fixed rather than growable.
    pages: u32,
    /// Size of the shadow-stack region in bytes, occupying `[0, stack_size)`.
    stack_size: u32,
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
            stack_size: PAGE_SIZE,
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
/// see the headroom invariant enforced by [`MemoryLayout::resolve`]. That is a stricter
/// bound than [`MAX_PAGES`] alone, and it is why a module may not declare the
/// whole address space.
const ADDRESS_SPACE: u64 = 1 << 32;

impl MemoryLayout {
    /// The layout a build asked for, with every dimension the request left unset
    /// taken from [`Self::default`].
    ///
    /// This is the checked constructor: partial specification is meaningful — a
    /// project that sets only `pages` wants the default stack inside a larger
    /// memory — so the filling happens before the check rather than after, and a
    /// request is judged as the whole layout it produces. That ordering is what
    /// lets a single well-formed number still be rejected: a 128 KiB stack is
    /// fine on its own terms and impossible inside the default one page.
    ///
    /// `surface` selects only the spelling a rejection names. The same two
    /// numbers are accepted or refused identically whether they came from a
    /// manifest or a command line, which is the property that makes this the one
    /// definition of a legal layout.
    ///
    /// # Errors
    ///
    /// Returns the violated invariant, rendered against `surface`.
    pub fn resolve(
        pages: Option<u32>,
        stack_size: Option<u32>,
        surface: MemoryLayoutSource,
    ) -> Result<Self, MemoryLayoutError> {
        let defaults = Self::default();
        let layout = Self {
            pages: pages.unwrap_or(defaults.pages),
            stack_size: stack_size.unwrap_or(defaults.stack_size),
        };
        layout
            .validate()
            .map_err(|reason| MemoryLayoutError { reason, surface })?;
        Ok(layout)
    }

    /// Linear memory size in 64 KiB pages.
    #[must_use]
    pub fn pages(self) -> u32 {
        self.pages
    }

    /// Size of the shadow-stack region in bytes, occupying `[0, stack_size)`.
    #[must_use]
    pub fn stack_size(self) -> u32 {
        self.stack_size
    }

    /// Checks that the two sizes describe a linear memory a module can actually
    /// declare and code generation can actually address.
    ///
    /// Private because [`Self::resolve`] is the only caller that can exist: a
    /// value of this type has already passed here, so a public re-check would
    /// invite consumers to guard against a state the type forbids.
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
    fn validate(self) -> Result<(), String> {
        let Self { pages, stack_size } = self;
        let page_size = u64::from(PAGE_SIZE);
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
        if stack_size % FRAME_ALIGNMENT != 0 {
            return Err(format!(
                "the shadow stack size must be a multiple of the {FRAME_ALIGNMENT}-byte frame \
                 alignment, because frame sizes are rounded to it and the stack top must land on \
                 that grid, but {stack_size} bytes were requested"
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
    /// The conversion is lossless for every layout [`Self::resolve`] accepts,
    /// which is what bounds `stack_size` by [`i32::MAX`].
    #[must_use]
    pub fn stack_pointer_init(self) -> i32 {
        self.stack_size.cast_signed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn abi_version_is_one_dot_three() {
        assert_eq!(COMPILER_ABI_MAJOR, 1);
        assert_eq!(COMPILER_ABI_MINOR, 3);
    }

    #[test]
    fn every_feature_round_trips_through_its_name() {
        for feature in WasmFeatureName::ALL {
            assert_eq!(WasmFeatureName::from_name(feature.as_str()), Some(feature));
        }
    }

    #[test]
    fn bulk_memory_is_spelled_kebab_case() {
        assert_eq!(WasmFeatureName::BulkMemory.as_str(), "bulk-memory");
    }

    #[test]
    fn name_matching_is_case_sensitive_and_untrimmed() {
        // A near-miss must not resolve: a typo that silently changed the
        // instruction level of a shipped artifact would be worse than an error.
        for near_miss in [
            "Bulk-Memory",
            "BULK-MEMORY",
            "bulk_memory",
            " bulk-memory",
            "bulk-memory ",
        ] {
            assert_eq!(
                WasmFeatureName::from_name(near_miss),
                None,
                "`{near_miss}` must not resolve"
            );
        }
    }

    #[test]
    fn empty_request_resolves_to_no_features() {
        assert_eq!(
            resolve_wasm_features(&[], WasmFeatureSource::Manifest),
            Ok(Vec::new())
        );
    }

    #[test]
    fn valid_request_resolves_in_input_order() {
        assert_eq!(
            resolve_wasm_features(&entries(&["bulk-memory"]), WasmFeatureSource::Manifest),
            Ok(vec![WasmFeatureName::BulkMemory])
        );
    }

    #[test]
    fn instruction_name_suggests_its_proposal() {
        let err = resolve_wasm_features(&entries(&["memory.fill"]), WasmFeatureSource::Manifest)
            .expect_err("an instruction name is not a feature");
        assert_eq!(
            err,
            WasmFeatureError::InstructionName {
                entry: "memory.fill".to_string(),
                proposal: WasmFeatureName::BulkMemory,
                surface: WasmFeatureSource::Manifest,
            }
        );
    }

    #[test]
    fn every_mapped_instruction_names_a_supported_proposal() {
        // The table may only suggest names the vocabulary accepts, otherwise the
        // did-you-mean would hand the user a second error.
        for (instruction, proposal) in INSTRUCTION_TO_PROPOSAL {
            assert_eq!(
                WasmFeatureName::from_name(proposal.as_str()),
                Some(*proposal),
                "`{instruction}` suggests an unsupported proposal"
            );
        }
    }

    #[test]
    fn inherent_feature_is_rejected_with_its_reason() {
        let err =
            resolve_wasm_features(&entries(&["mutable-globals"]), WasmFeatureSource::Manifest)
                .expect_err("an always-on feature cannot be requested");
        assert_eq!(
            err,
            WasmFeatureError::InherentFeature {
                entry: "mutable-globals".to_string(),
                surface: WasmFeatureSource::Manifest,
            }
        );
    }

    #[test]
    fn duplicate_entry_is_rejected() {
        let err = resolve_wasm_features(
            &entries(&["bulk-memory", "bulk-memory"]),
            WasmFeatureSource::Flag,
        )
        .expect_err("a feature may appear at most once");
        assert_eq!(
            err,
            WasmFeatureError::DuplicateFeature {
                entry: "bulk-memory".to_string(),
                surface: WasmFeatureSource::Flag,
            }
        );
    }

    #[test]
    fn unknown_entry_lists_the_supported_set() {
        let err = resolve_wasm_features(&entries(&["simd"]), WasmFeatureSource::Flag)
            .expect_err("an unsupported proposal is not in the vocabulary");
        assert_eq!(
            err,
            WasmFeatureError::UnknownFeature {
                entry: "simd".to_string(),
                surface: WasmFeatureSource::Flag,
            }
        );
    }

    #[test]
    fn padded_entry_is_rejected_and_says_so() {
        // Whitespace is rejected, never trimmed — but a trailing space in a TOML
        // string is invisible in the echoed entry, so the message names the cause.
        let err = resolve_wasm_features(&entries(&["bulk-memory "]), WasmFeatureSource::Manifest)
            .expect_err("a padded name must not resolve");
        let rendered = err.to_string();
        assert!(rendered.contains("surrounding whitespace"), "{rendered}");
        assert!(rendered.contains("write `bulk-memory`"), "{rendered}");
    }

    #[test]
    fn a_repeated_typo_is_reported_as_a_typo() {
        let err = resolve_wasm_features(&entries(&["simd", "simd"]), WasmFeatureSource::Flag)
            .expect_err("an unknown name is still unknown when repeated");
        assert!(
            matches!(err, WasmFeatureError::UnknownFeature { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn first_bad_entry_is_the_one_reported() {
        let err = resolve_wasm_features(
            &entries(&["memory.fill", "simd"]),
            WasmFeatureSource::Manifest,
        )
        .expect_err("the request is invalid");
        assert!(
            matches!(err, WasmFeatureError::InstructionName { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn source_selects_the_surface_the_message_names() {
        assert_eq!(
            WasmFeatureSource::Manifest.label(),
            "`[build] wasm-features`"
        );
        assert_eq!(WasmFeatureSource::Flag.label(), "`--wasm-features`");
    }

    #[test]
    fn feature_list_renders_sorted_and_comma_joined() {
        assert_eq!(
            render_feature_list(&[WasmFeatureName::BulkMemory]),
            "bulk-memory"
        );
        assert_eq!(render_feature_list(&[]), "");
    }

    /// Every variant's full rendering, pinned character for character.
    ///
    /// The `#[error(...)]` attributes are the only copy of this wording, and both
    /// front ends show it verbatim, so a reworded diagnostic is a user-visible
    /// change that has to be made deliberately here rather than drifting out of
    /// one caller.
    #[test]
    fn every_variant_renders_its_exact_wording() {
        let cases = [
            (
                WasmFeatureError::UnknownFeature {
                    entry: "simd".to_string(),
                    surface: WasmFeatureSource::Flag,
                },
                "Invalid `--wasm-features` entry `simd`: unknown WebAssembly feature. \
                 Supported features: `bulk-memory`.",
            ),
            (
                WasmFeatureError::UnknownFeature {
                    entry: "bulk-memory ".to_string(),
                    surface: WasmFeatureSource::Manifest,
                },
                "Invalid `[build] wasm-features` entry `bulk-memory `: unknown WebAssembly \
                 feature. Supported features: `bulk-memory`. Feature names are matched exactly \
                 and this entry has surrounding whitespace: write `bulk-memory`.",
            ),
            (
                WasmFeatureError::InstructionName {
                    entry: "memory.fill".to_string(),
                    proposal: WasmFeatureName::BulkMemory,
                    surface: WasmFeatureSource::Manifest,
                },
                "Invalid `[build] wasm-features` entry `memory.fill`: `memory.fill` is an \
                 instruction, not a feature. Features are named after the proposal that \
                 introduced them, which enables the whole instruction family at once — write \
                 `bulk-memory` instead.",
            ),
            (
                WasmFeatureError::InherentFeature {
                    entry: "mutable-globals".to_string(),
                    surface: WasmFeatureSource::Manifest,
                },
                "Invalid `[build] wasm-features` entry `mutable-globals`: `mutable-globals` is \
                 always enabled and cannot be requested. Every Inference module that allocates \
                 a stack frame uses a mutable `__stack_pointer` global, so this is part of the \
                 baseline rather than an opt-in — remove the entry.",
            ),
            (
                WasmFeatureError::DuplicateFeature {
                    entry: "bulk-memory".to_string(),
                    surface: WasmFeatureSource::Flag,
                },
                "Invalid `--wasm-features`: `bulk-memory` is listed more than once. Each \
                 feature may appear at most once — remove the duplicate.",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    /// The message helpers must render their variant and nothing else, so a caller
    /// that wants a `String` and one that propagates the error read the same.
    #[test]
    fn message_helpers_render_their_variant() {
        let surface = WasmFeatureSource::Manifest;
        assert_eq!(
            unknown_feature_message("simd", surface),
            WasmFeatureError::UnknownFeature {
                entry: "simd".to_string(),
                surface
            }
            .to_string()
        );
        assert_eq!(
            instruction_name_message("memory.fill", WasmFeatureName::BulkMemory, surface),
            WasmFeatureError::InstructionName {
                entry: "memory.fill".to_string(),
                proposal: WasmFeatureName::BulkMemory,
                surface
            }
            .to_string()
        );
        assert_eq!(
            inherent_feature_message("mutable-globals", surface),
            WasmFeatureError::InherentFeature {
                entry: "mutable-globals".to_string(),
                surface
            }
            .to_string()
        );
        assert_eq!(
            duplicate_feature_message("bulk-memory", surface),
            WasmFeatureError::DuplicateFeature {
                entry: "bulk-memory".to_string(),
                surface
            }
            .to_string()
        );
    }

    /// `ALL` must list every variant, or a name becomes unrequestable while still
    /// existing in the vocabulary — and neither exhaustive match downstream would
    /// notice, because both iterate `ALL`.
    ///
    /// `every_variant` is the ground truth: adding a variant makes the match in
    /// `name_is_a_known_variant` non-exhaustive, so the compiler forces an edit to
    /// this test, and the length assertion then fails until `ALL` lists it too.
    #[test]
    fn all_lists_every_variant() {
        fn name_is_a_known_variant(feature: WasmFeatureName) {
            match feature {
                WasmFeatureName::BulkMemory => {}
            }
        }

        let every_variant = [WasmFeatureName::BulkMemory];
        for feature in every_variant {
            name_is_a_known_variant(feature);
            assert!(
                WasmFeatureName::ALL.contains(&feature),
                "`{}` is a variant but is missing from ALL",
                feature.as_str()
            );
        }
        assert_eq!(
            WasmFeatureName::ALL.len(),
            every_variant.len(),
            "ALL and the known-variant list disagree in length"
        );
    }

    #[test]
    fn supported_listing_covers_every_variant() {
        let listing = supported_features_listing();
        for feature in WasmFeatureName::ALL {
            assert!(
                listing.contains(feature.as_str()),
                "`{}` missing from the supported listing",
                feature.as_str()
            );
        }
    }

    /// Both dimensions given, for the cases where the point is the resulting
    /// layout rather than which keys the request left unset.
    fn layout(pages: u32, stack_size: u32) -> Result<MemoryLayout, MemoryLayoutError> {
        MemoryLayout::resolve(Some(pages), Some(stack_size), MemoryLayoutSource::Flag)
    }

    #[test]
    fn default_layout_is_one_page_of_stack() {
        assert_eq!(MemoryLayout::default().pages(), 1);
        assert_eq!(MemoryLayout::default().stack_size(), 65_536);
    }

    /// `Default` bypasses the checked constructor, so nothing but this says the
    /// value it hands out is one the constructor would have accepted.
    #[test]
    fn the_default_is_a_layout_the_constructor_accepts() {
        assert_eq!(
            MemoryLayout::resolve(None, None, MemoryLayoutSource::Manifest),
            Ok(MemoryLayout::default())
        );
    }

    #[test]
    fn default_stack_pointer_starts_past_the_last_stack_address() {
        assert_eq!(MemoryLayout::default().stack_pointer_init(), 65_536);
    }

    /// An unset dimension takes the default and the set one is honored, in both
    /// directions. Partial specification is the common case — a project that
    /// wants a bigger memory has no reason to restate the stack size — so each
    /// key has to be independently settable.
    #[test]
    fn an_unset_dimension_is_filled_from_the_default() {
        let pages_only = MemoryLayout::resolve(Some(4), None, MemoryLayoutSource::Manifest)
            .expect("four pages holds the default stack");
        assert_eq!(pages_only.pages(), 4);
        assert_eq!(pages_only.stack_size(), MemoryLayout::default().stack_size());

        let stack_only = MemoryLayout::resolve(None, Some(32_768), MemoryLayoutSource::Manifest)
            .expect("half a page of stack fits the default page");
        assert_eq!(stack_only.pages(), MemoryLayout::default().pages());
        assert_eq!(stack_only.stack_size(), 32_768);
    }

    /// Filling happens before checking, so a partial request is judged as the
    /// whole layout it produces rather than on the one number it names.
    ///
    /// 128 KiB is a perfectly ordinary stack size — `pages = 4, stack-size =
    /// 131072` is accepted below — and it is impossible here only because the
    /// unstated page count defaulted to one. A constructor that validated the
    /// stated key alone would accept this and emit a stack twice the size of the
    /// memory holding it.
    #[test]
    fn a_partial_request_is_checked_against_the_layout_it_completes_to() {
        let err = MemoryLayout::resolve(None, Some(131_072), MemoryLayoutSource::Manifest)
            .expect_err("a 128 KiB stack cannot live in the default single page");
        assert!(
            err.reason.contains("does not fit in the linear memory"),
            "{err}"
        );
        assert!(
            MemoryLayout::resolve(Some(4), Some(131_072), MemoryLayoutSource::Manifest).is_ok(),
            "the same stack size is fine once the memory is large enough"
        );
    }

    /// Each invariant rejects on its own. The expectations pin both the invariant
    /// that fired and the value it names, because several messages would
    /// otherwise be satisfied by the same number: a layout that breaks one rule
    /// must not be reported under another.
    ///
    /// These read as "this cannot be built" rather than "this can be built and
    /// is invalid", which is the whole point of the private fields: outside this
    /// crate the rejected values have no inhabited representation at all.
    #[test]
    fn the_constructor_rejects_each_broken_invariant() {
        let cases = [
            (0, 65_536, "at least one 64 KiB page", "0 pages"),
            (65_537, 65_536, "limited to 65536 pages", "65537 pages"),
            (1, 0, "at least one frame wide", "0 bytes"),
            (
                1,
                1_000,
                "multiple of the 16-byte frame alignment",
                "1000 bytes",
            ),
            (
                1,
                131_072,
                "does not fit in the linear memory",
                "131072 bytes",
            ),
            (
                65_536,
                2_147_483_664,
                "signed 32-bit constant",
                "2147483664 bytes",
            ),
            // A memory filling the whole address space leaves a wrapped stack
            // pointer nowhere out of bounds to land, so the overflow trap is
            // gone. Every other rule here is satisfied.
            (
                65_536,
                65_536,
                "32-bit address space",
                "4295032832 bytes",
            ),
        ];
        for (pages, stack_size, invariant, value) in cases {
            let err = layout(pages, stack_size)
                .expect_err(&format!("{pages} pages / {stack_size} bytes must be rejected"));
            assert!(
                err.reason.contains(invariant),
                "{pages} pages / {stack_size} bytes was rejected with `{err}`, which is not the \
                 `{invariant}` rule"
            );
            assert!(
                err.reason.contains(value),
                "{pages} pages / {stack_size} bytes was rejected with `{err}`, which does not \
                 name `{value}`"
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
    fn the_constructor_accepts_the_extremes_of_the_admissible_range() {
        assert!(layout(65_535, 16).is_ok());
        assert!(layout(2, 16).is_ok());
    }

    /// The headroom rule is exactly `memory_bytes + stack_size <= 2^32`, so a
    /// layout one byte-grid step either side of the boundary must land on
    /// opposite verdicts. Without this pair the rule could be off by a whole
    /// page and every other test would still pass.
    #[test]
    fn the_address_space_headroom_boundary_is_exact() {
        let fits = layout(65_535, 65_536).expect("this layout sits exactly on the boundary");
        assert_eq!(
            u64::from(fits.pages()) * 65_536 + u64::from(fits.stack_size()),
            1 << 32,
            "this case is meant to sit exactly on the boundary"
        );

        assert!(
            layout(fits.pages(), fits.stack_size() + 16).is_err(),
            "one frame past the boundary must be rejected"
        );
    }

    #[test]
    fn memory_source_selects_the_spelling_the_message_names() {
        assert_eq!(
            MemoryLayoutSource::Manifest.label(),
            "`[memory] pages` / `[memory] stack-size`"
        );
        assert_eq!(
            MemoryLayoutSource::Flag.label(),
            "`--memory-pages` / `--stack-size`"
        );
    }

    /// The rendering is pinned character for character, for the same reason the
    /// feature diagnostics are: the `#[error(...)]` attribute is the only copy of
    /// this wording and both front ends show it verbatim.
    ///
    /// The two surfaces carry the same `reason` so the pair also pins that the
    /// verdict is surface-independent — only the spelling changes.
    #[test]
    fn the_memory_error_renders_its_exact_wording() {
        let manifest = MemoryLayout::resolve(Some(0), None, MemoryLayoutSource::Manifest)
            .expect_err("a zero-page memory is rejected");
        assert_eq!(
            manifest.to_string(),
            "Invalid `[memory] pages` / `[memory] stack-size`: linear memory must be at least \
             one 64 KiB page, but 0 pages were requested"
        );

        let flag = MemoryLayout::resolve(Some(0), None, MemoryLayoutSource::Flag)
            .expect_err("a zero-page memory is rejected");
        assert_eq!(
            flag.to_string(),
            "Invalid `--memory-pages` / `--stack-size`: linear memory must be at least one \
             64 KiB page, but 0 pages were requested"
        );
        assert_eq!(
            manifest.reason, flag.reason,
            "the same numbers must be refused for the same reason on either surface"
        );
    }
}
