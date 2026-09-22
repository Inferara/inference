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
//! # Compilation target vocabulary
//!
//! [`TargetName`] is the set of runtimes a build may target. It is the axis of
//! *what the module is built for*, not of which back end produces it: `wasm32`
//! is the generic value, a module for any WebAssembly embedder that imposes no
//! ABI of its own, while every further name stands for one runtime with its own
//! calling convention or acceptance rules. The set is closed rather than a
//! triple parsed at the boundary, because each name is something the compiler
//! has to have been taught.
//!
//! [`resolve_target`] is the shared validation, so `infs` (reading
//! `Inference.toml`) and `infc` (reading `--target`) reject the same spellings
//! with the same words, and [`TargetName::abi_minor`] records the `infc` ABI
//! minor each name became requestable at — per name, because a caller gating a
//! forward on a single floor would wave a newer name past a compiler that
//! cannot build for it.
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

pub use crate::errors::{MemoryLayoutError, TargetError, WasmFeatureError};

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
///
/// Minor 4 adds the additive `--adopt-external-specs` flag to `infc`, carrying a
/// linked library's universal proof obligations into the program's proof
/// artifact. It is backward compatible in the same sense as the minors above:
/// omitting it yields the artifact every earlier minor produced, in which a
/// library's obligations are absent. The reverse pairing is the one callers must
/// gate on, and it is the least visible of the four. A minor-3 `infc` cannot
/// honor the request and has no way to report that it ignored one, and the
/// difference the request makes is *which theorems the `.v` states* — a missing
/// one looks exactly like a proof artifact that was never asked for the
/// obligation. An `infs` that forwards must therefore confirm the minor it is
/// talking to and refuse, rather than write a proof artifact whose contents are
/// not the ones the manifest asked for.
///
/// Minor 5 adds the additive `--target <name>` flag to `infc`, naming the
/// runtime the emitted module is built for from the vocabulary [`TargetName`]
/// holds. It is backward compatible in the same sense as the minors above:
/// omitting the flag selects [`TargetName::DEFAULT`], which is what every
/// earlier minor built for, so a minor-4 `infs` still pairs with a minor-5
/// `infc`. The reverse pairing is the one callers must gate on, and it has to be
/// gated per *name* rather than on this single minor: each name enters the
/// vocabulary at its own minor, recorded by [`TargetName::abi_minor`], so one
/// floor set at the earliest of them would wave a later name past a compiler
/// that cannot build for it. What an ungated forward costs depends on the name
/// it drops. For a name whose emission is byte-for-byte the default's, the
/// artifact still runs and what is lost is that target's conformance check,
/// which silently never ran. For a name whose emission differs, what is lost is
/// the bytes: the build ships a module in the default's shape, which the runtime
/// the manifest named cannot invoke. An `infs` that forwards must therefore
/// confirm the minor the name it is forwarding needs, and refuse rather than
/// build for a target it cannot ask for. The gate belongs only to a name whose
/// omission would change the artifact: a forward of [`TargetName::DEFAULT`] is
/// *omitted* rather than gated, because dropping it selects the same target the
/// older compiler already builds for, and gating it would turn every project
/// build against an older `infc` into the hard error this flag was designed not
/// to cause.
///
/// Minor 6 adds no flag. It adds the name `stellar` to the vocabulary
/// [`TargetName`] holds, and with it the first target whose artifact is not the
/// default's — a module wrapped in a contract calling convention rather than
/// the plain WebAssembly every earlier minor emitted. It is backward compatible
/// in the same sense as the minors above: a build that names no target, or
/// names `wasm32`, gets exactly the artifact minor 5 produced, so a minor-5
/// `infs` still pairs with a minor-6 `infc`. The reverse pairing is the one
/// callers must gate on, and it is the case the per-name accessor exists for. A
/// forward that *reaches* a minor-5 `infc` fails loudly, because that compiler
/// parses `--target` and has no such name; what a caller must not do is decide
/// the flag is unnecessary and omit it, because then the build succeeds and
/// ships a module in the default's shape, which the Stellar runtime cannot
/// invoke. So the gate is on [`TargetName::Stellar`]'s own
/// [`TargetName::abi_minor`] — neither on this constant, which any later
/// additive flag will bump past it, nor on the minor `--target` itself entered
/// at, which is five and would wave this name through.
///
/// Minor 7 adds no flag either. It adds the name `spacewasm` to the vocabulary
/// [`TargetName`] holds — the second name-only minor, and the first name whose
/// artifact *is* the default's: nothing on the emission path reads a target, so
/// a `spacewasm` build and a `wasm32` build of one source are the same bytes.
/// What this name selects is an envelope rather than an artifact. It is
/// backward compatible in the same sense as the minors above: a build that
/// names no target, or names `wasm32`, gets exactly the artifact minor 6
/// produced. The reverse pairing is gated for the reason minor 5 states, and
/// this is the first name to land on the cheaper of the two consequences that
/// sentence describes rather than the expensive one. A forward that *reaches* a
/// minor-6 `infc` fails loudly, on a name that compiler does not have; a
/// forward a caller decides is unnecessary and *omits* costs nothing in the
/// bytes and costs the whole of what the name is for, because the envelope —
/// and the conformance check that lands with it — is then never applied to the
/// module that ships. So the gate is on [`TargetName::SpaceWasm`]'s own
/// [`TargetName::abi_minor`] like every other name's, even though the artifact
/// it protects is one an older compiler would have produced byte for byte.
///
/// Minor 8 adds the additive `--host-imports=<list>` flag to `infc`, naming the
/// host functions a build may bind as `module.field` pairs — the allowlist a
/// program's `use … from host::<module>;` clauses are held to. It is backward
/// compatible in the same sense as the minors above: omitting the flag applies
/// no policy, which is what every earlier minor did with a host binding, so a
/// minor-7 `infs` still pairs with a minor-8 `infc`, and loses nothing by it: a
/// minor-7 `infs` predates the `Inference.toml [host-imports]` table, so it can
/// hold no host-import policy to drop. The pairing callers must gate on is an
/// `infs` that holds a policy talking to a minor-7 `infc`, and it is the most
/// expensive ungated forward of the eight. A forward that *reaches* a minor-7
/// `infc` fails loudly on a flag that compiler does not parse; what a caller
/// must not do is decide the flag is unnecessary and omit it, because the build
/// then succeeds and ships an artifact that asks an embedder for functions the
/// project never admitted — the allowlist simply did not run. Nothing in the
/// bytes records the difference: an artifact built under an allowlist and one
/// built under none are the same module, so unlike a dropped `--target`, there
/// is no wrong artifact to find later. What a skipped allowlist does leave is a
/// positive marker rather than a missing check line — the `host imports:`
/// inventory `infc` prints picks up a `(no allowlist)` qualifier — and that
/// line is printed only by a program that binds at least one host import, and
/// lives no longer than the build's output does. The empty spelling is the
/// sharpest case. `--host-imports=` is a policy that admits nothing, and a
/// caller that drops it turns a project declaring it binds no host functions at
/// all into one that binds whatever its source happens to say. So the gate is
/// on this constant, as it is for every flag minor: an `infs` that forwards
/// must confirm it is talking to a minor-8 `infc` and refuse, never drop the
/// flag and build.
pub const COMPILER_ABI_MINOR: u32 = 8;

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

/// A runtime a build may target, as it is named in a project's `Inference.toml`
/// and on the `infc --target` command line.
///
/// The axis is the runtime the module is built for, not a compiler back end and
/// not a target triple. [`Self::Wasm32`] is the generic value — a module for any
/// WebAssembly embedder that imposes no ABI of its own — and every further name
/// stands for one specific runtime, which is why this is a closed vocabulary
/// rather than a triple parsed at the boundary.
///
/// It is deliberately separate from the emission-side target in
/// `inference-wasm-codegen`, for the reason [`WasmFeatureName`] is separate from
/// that crate's `EmitFeatures`: this is the set a user may *request*, that one is
/// the set code generation knows how to *produce*, and the two are not the same
/// set while a target is being built. `inference-wasm-codegen` carries the
/// cross-check that maps one onto the other.
///
/// Adding a variant carries four obligations, each of which fails a compile or a
/// test rather than resting on review: the variant records its own
/// [`Self::abi_minor`], [`COMPILER_ABI_MINOR`] is bumped to that value, every
/// exhaustive match from a name onto an emission target gains an arm, and each
/// accessor below decides the new name explicitly. The accessors are matches
/// rather than `matches!(self, Self::Wasm32)` for that last reason alone: the
/// concise form compiles unchanged for a new variant and hands it `false`, so a
/// name that should have been allowed something would be refused it with nobody
/// having decided so. The two that answer with a sentence rather than a `bool`
/// are held to the same rule for the same reason, and answer `Option` so that a
/// `None` arm is a decision that this name has nothing extra to say rather than
/// a variant nobody reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetName {
    /// General-purpose WebAssembly, for an embedder that imposes no ABI of its
    /// own. The default, and what every build produced before a target could be
    /// named.
    Wasm32,

    /// A Stellar smart contract, for the Soroban host.
    ///
    /// The runtime imposes a calling convention of its own — every exported
    /// method takes and returns the host's 64-bit tagged word — so this is the
    /// first name whose artifact is not the default's. What a build for it may
    /// contain is correspondingly narrower: no proof mode, no post-MVP
    /// instruction family, and an exported function carrying only the scalar
    /// types the convention can encode without a host object. Code generation
    /// owns the last of those; the two here are the ones a front end can answer
    /// before spawning a compiler at all.
    Stellar,

    /// A module for the SpaceWasm flight interpreter.
    ///
    /// The runtime is NASA JPL's `no_std` WebAssembly 1.0 interpreter for
    /// flight software, and it imposes no ABI of its own: the artifact is the
    /// default target's, byte for byte, with nothing appended and nothing
    /// rewritten. This is the first name that selects an envelope rather than
    /// an artifact, which is why it is the counterexample to reading
    /// "non-default target" as "different bytes".
    ///
    /// What the envelope narrows, of the things a front end holding only the
    /// name can answer: no proof mode, because the interpreter's decoder does
    /// not define the custom `0xfc` instructions, and no post-MVP instruction
    /// family, because it implements none of the proposals that add one. The
    /// decode-time maxima the interpreter also enforces — parameter and local
    /// word counts, name lengths, host arity — are properties of the finished
    /// module rather than of the request, so nothing here can answer them.
    SpaceWasm,
}

impl TargetName {
    /// Every requestable target, in canonical order. The rendered supported-set
    /// listing in diagnostics comes from here, so a new variant surfaces in every
    /// "unknown target" message with no further edit.
    ///
    /// Prose is not rendered from here. The set is restated in documentation no
    /// test can make red, and [`RESERVED_TARGET_NAMES`] carries the inventory of
    /// where, which a name added here has to walk.
    pub const ALL: [Self; 3] = [Self::Wasm32, Self::Stellar, Self::SpaceWasm];

    /// The target a build gets when it names none, on either surface.
    ///
    /// A single constant rather than a `Default` impl per surface: the manifest
    /// default and the flag default have to be the same target, and a second
    /// spelling of it is a second thing to keep in step.
    pub const DEFAULT: Self = Self::Wasm32;

    /// The target as it is written in `Inference.toml` and on the `--target`
    /// command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wasm32 => "wasm32",
            Self::Stellar => "stellar",
            Self::SpaceWasm => "spacewasm",
        }
    }

    /// The target written exactly as `name`, or `None`.
    ///
    /// The inverse of [`Self::as_str`]: matching is exact and case-sensitive, and
    /// no whitespace is trimmed. Accepting a near-miss would build the artifact
    /// for a runtime the author did not name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == name)
    }

    /// The `infc` ABI minor at which this target became requestable.
    ///
    /// Per name rather than a single minimum: names enter the vocabulary at
    /// different minors, so a caller that gated a forward on one floor would
    /// either refuse pairings that work or, worse, forward a name to a compiler
    /// that parses the flag and cannot build for it. A caller asks
    /// "does this `infc` reach *this name*", never "does it reach targets".
    #[must_use]
    pub fn abi_minor(self) -> u32 {
        match self {
            Self::Wasm32 => 5,
            Self::Stellar => 6,
            Self::SpaceWasm => 7,
        }
    }

    /// Whether a build for this target may run in proof mode.
    ///
    /// Proof mode emits Inference's custom `0xfc` non-deterministic
    /// instructions, which no runtime but a general-purpose WebAssembly
    /// embedder under our own tooling can decode. A target that refuses them
    /// refuses the mode.
    ///
    /// Mirrored by the emission-side target in `inference-wasm-codegen`, which
    /// is where the refusal is enforced; this copy is what lets a front end
    /// holding only a name — `infs` reading a manifest, before any compiler is
    /// spawned — reject the combination in its own words. The two are held to
    /// each other by that crate's mirror test, so the copy cannot drift into a
    /// second opinion.
    #[must_use]
    pub fn supports_proof_mode(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Whether a build for this target may request a post-MVP instruction
    /// family.
    ///
    /// The same mirror rule as [`Self::supports_proof_mode`]: the emission-side
    /// target decides, this copy exists for a front end that has only the name.
    #[must_use]
    pub fn permits_bulk_memory(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Whether a module built for this target can be invoked by a plain
    /// WebAssembly runtime — one that calls an export by name and passes each
    /// argument as a value of the declared parameter type.
    ///
    /// This is the question `infs run` asks before handing an artifact to
    /// `wasmtime`, and it has no emission-side counterpart: whether a module
    /// *runs* under a general-purpose runtime is a property of the calling
    /// convention its runtime imposes, which code generation never sees. A
    /// target that imposes one answers `false` — not because the module is
    /// invalid WebAssembly, but because invoking it this way returns a wrong
    /// answer rather than an error.
    ///
    /// A target that adds no convention answers `true` even when it is not the
    /// default: the SpaceWasm artifact is the default's bytes and its `main`
    /// keeps the `argc, argv` shape, so a general-purpose runtime can invoke it.
    ///
    /// That is an answer about the calling convention and nothing else. The
    /// runtime a `true` here reaches is not the target's runtime, so running the
    /// module locally exercises the module and not the environment it was built
    /// for: none of that environment's decode-time maxima, and nothing else it
    /// does differently, is checked by running it here.
    #[must_use]
    pub fn runs_under_a_plain_wasm_runtime(self) -> bool {
        match self {
            Self::Wasm32 | Self::SpaceWasm => true,
            Self::Stellar => false,
        }
    }

    /// What a build loses if a caller drops this target instead of forwarding
    /// it, as a sentence appended to the refusal that would otherwise report
    /// only version arithmetic.
    ///
    /// Per name because the loss is per name and no reader can infer it from
    /// the arithmetic: for one target the artifact comes out the wrong shape
    /// entirely, for another it comes out byte-identical with the whole reason
    /// the name was written never applied. A refusal that names the minor but
    /// not the stake reads as toolchain pedantry.
    ///
    /// The sentence names no surface. The two ways to select a target are not
    /// spelled alike — `infc` takes a flag, `infs` takes a manifest key — and
    /// the message this is appended to has already named whichever the user
    /// wrote, so a clause that named one of them would be wrong wherever the
    /// other was.
    ///
    /// [`Self::DEFAULT`] is never forwarded — dropping it selects the same
    /// target — so it answers `None`: this name has nothing to add, as a
    /// decision rather than as an empty string a caller has to remember not to
    /// append a space in front of.
    #[must_use]
    pub fn unforwarded_consequence(self) -> Option<&'static str> {
        match self {
            Self::Wasm32 => None,
            Self::Stellar => Some(
                "Building without it would produce a plain WebAssembly module: no \
                 value-ABI wrappers over the exported methods and no environment-metadata \
                 section, so the artifact would not be a contract and a Soroban host would \
                 refuse to upload it.",
            ),
            Self::SpaceWasm => Some(
                "Building without it would produce the same bytes, since nothing on the \
                 emission path reads a target — and would drop this target's acceptance \
                 envelope with it, so nothing would hold the module that ships to what the \
                 flight interpreter decodes.",
            ),
        }
    }

    /// The target-specific sentence a proof-mode refusal ends with, or `None`
    /// when the generic remediation is the whole of it.
    ///
    /// The generic remediation — set `mode = "compile"`, or build for a target
    /// that supports proof mode — is true everywhere and actionable nowhere in
    /// particular. For a target whose artifact *is* the default's it is worse
    /// than unhelpful: it reads as "this program cannot be proved" when what is
    /// true is that the proof is one command away and describes these very
    /// bytes, so that name earns the sentence saying so.
    ///
    /// A target whose artifact is a *rewrite* of the default's earns no such
    /// sentence, and its `None` arm is the decision rather than an oversight.
    /// There is a proof route for it too, but the relationship it rests on is
    /// "rewritten from" rather than "equal to" — which takes the procedure the
    /// book's Compilation Targets chapter sets out, not a clause appended to a
    /// manifest error.
    ///
    /// Kept beside [`Self::unforwarded_consequence`] and held to the same
    /// exhaustive-match rule: a name added to the vocabulary states which of the
    /// two situations it is in rather than inheriting silence. It names no
    /// selection surface for the same reason that clause names none: this one is
    /// appended today to a manifest error and the emission-side refusal of the
    /// same pairing is a flag error, so a sentence naming either would be wrong
    /// in the other.
    #[must_use]
    pub fn proof_refusal_remediation(self) -> Option<&'static str> {
        match self {
            Self::Wasm32 | Self::Stellar => None,
            Self::SpaceWasm => Some(
                "No separate proof build is needed for this target: nothing on the emission \
                 path reads a target, so its compile-mode bytes are the `wasm32` build's. \
                 Prove the program in a `wasm32` build and deploy the artifact this build \
                 produces.",
            ),
        }
    }
}

/// Names that are not requestable and earn a dedicated diagnostic anyway,
/// because the generic "unknown target" would be misleading about them.
///
/// The single entry is the former name of [`TargetName::Stellar`], and its
/// message redirects to the current spelling. The message reads the name off
/// this slice rather than spelling it into prose, so promoting or retiring one
/// is an edit here rather than to a sentence.
///
/// The wording on [`TargetError::ReservedTarget`] says "former name", which is
/// true of everything in this slice today and is the obligation an addition
/// takes on: a name added here for some other reason — a target the emitter
/// carries before a user may ask for it, say — needs its own variant rather
/// than this one, or the message will tell the user to write a spelling that
/// does not exist.
///
/// Deleting an entry is only most of the edit. The accepted-target set is also
/// restated in prose that no test can make red, and each of these has to move in
/// the same change:
///
/// - `apps/infs/docs/inference-toml.md` — the accepted values listed under the
///   `[build]` `target` field.
/// - `book/src/projects-and-the-infs-toolchain.md` — the `target` row of the
///   manifest field table, the `--target` row of the `infc` flag table, the
///   scaffolded `Inference.toml` it reproduces (an abridged restatement of the
///   scaffolder's comment below), and the paragraph on case sensitivity that
///   names the rejected spellings.
/// - `apps/infs/src/project/scaffold.rs` — the `[build]` comment the scaffolder
///   writes into a new project's `Inference.toml`.
pub const RESERVED_TARGET_NAMES: &[&str] = &["soroban"];

/// Which surface a target was named on, so a diagnostic can name the exact thing
/// the user has to edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSource {
    /// The `target` key of a project's `Inference.toml` `[build]` table.
    Manifest,
    /// The `--target` flag on an `infc` command line.
    Flag,
}

impl TargetSource {
    /// The backtick-quoted surface name a message points the user at.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manifest => "`[build] target`",
            Self::Flag => "`--target`",
        }
    }
}

/// The supported set as a diagnostic renders it: backtick-quoted names, comma
/// separated, in canonical order.
#[must_use]
pub fn supported_targets_listing() -> String {
    TargetName::ALL
        .iter()
        .map(|t| format!("`{}`", t.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves the target named on `source` into the vocabulary, or returns the one
/// diagnostic that rejects it.
///
/// This is the whole validation both front ends run — not just the wording — so
/// the order the failure families are checked in is decided once. The vocabulary
/// is consulted first and [`RESERVED_TARGET_NAMES`] second, which is what makes
/// promoting a reserved name a one-line deletion: until the name is requestable
/// it gets the reserved sentence, and the moment it is, the vocabulary answers
/// first.
///
/// Matching is exact and case-sensitive and no whitespace is trimmed, for the
/// reason [`TargetName::from_name`] gives.
///
/// # Errors
///
/// Returns [`TargetError::ReservedTarget`] for a name in
/// [`RESERVED_TARGET_NAMES`], and [`TargetError::UnknownTarget`] for anything
/// else outside the vocabulary.
pub fn resolve_target(entry: &str, source: TargetSource) -> Result<TargetName, TargetError> {
    if let Some(target) = TargetName::from_name(entry) {
        return Ok(target);
    }
    if RESERVED_TARGET_NAMES.contains(&entry) {
        return Err(TargetError::ReservedTarget {
            entry: entry.to_string(),
            surface: source,
        });
    }
    Err(TargetError::UnknownTarget {
        entry: entry.to_string(),
        surface: source,
    })
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
    fn abi_version_is_one_dot_eight() {
        assert_eq!(COMPILER_ABI_MAJOR, 1);
        assert_eq!(COMPILER_ABI_MINOR, 8);
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
    // Compilation target vocabulary ---

    #[test]
    fn every_target_round_trips_through_its_name() {
        for target in TargetName::ALL {
            assert_eq!(TargetName::from_name(target.as_str()), Some(target));
        }
    }

    #[test]
    fn the_default_target_is_wasm32() {
        assert_eq!(TargetName::DEFAULT, TargetName::Wasm32);
        assert_eq!(TargetName::DEFAULT.as_str(), "wasm32");
    }

    /// `spacewasm` is the first name whose natural CamelCase rendering differs
    /// from its wire spelling by more than a leading capital, so the separator
    /// spellings a reader might reach for — `space-wasm`, `space_wasm` — are
    /// listed beside the case variants. The wire spelling is one word.
    #[test]
    fn target_matching_is_case_sensitive_and_untrimmed() {
        for near_miss in [
            "Wasm32",
            "WASM32",
            "wasm_32",
            " wasm32",
            "wasm32 ",
            "Stellar",
            "STELLAR",
            " stellar",
            "stellar ",
            "SpaceWasm",
            "SPACEWASM",
            "space-wasm",
            "space_wasm",
            " spacewasm",
            "spacewasm ",
        ] {
            assert_eq!(
                TargetName::from_name(near_miss),
                None,
                "`{near_miss}` must not resolve"
            );
        }
    }

    /// The minor each name became requestable at, recorded by hand.
    ///
    /// This is the second opinion that turns [`TargetName::abi_minor`] into a
    /// decision rather than a copy: adding a variant leaves it uncovered here
    /// until someone writes its minor down. It is deliberately not derived from
    /// [`COMPILER_ABI_MINOR`], which *any* additive `infc` flag bumps — the four
    /// minors before this one had nothing to do with targets — so an unrelated
    /// bump must not turn this red.
    const TARGET_ENTRY_MINORS: &[(TargetName, u32)] = &[
        (TargetName::Wasm32, 5),
        (TargetName::Stellar, 6),
        (TargetName::SpaceWasm, 7),
    ];

    /// The safety property: no name may claim a minor this compiler does not
    /// advertise. A name whose minor ran ahead of [`COMPILER_ABI_MINOR`] would
    /// have callers gate a forward on a floor no released compiler reports, so
    /// every pairing would be refused.
    #[test]
    fn no_target_claims_a_minor_the_compiler_does_not_advertise() {
        for target in TargetName::ALL {
            assert!(
                target.abi_minor() <= COMPILER_ABI_MINOR,
                "`{}` claims a minor this compiler does not advertise",
                target.as_str()
            );
        }
    }

    /// The record must cover the vocabulary exactly, both ways: every name has a
    /// minor written down, and nothing is written down that is not a name.
    #[test]
    fn every_target_records_the_minor_it_entered_at() {
        assert_eq!(
            TARGET_ENTRY_MINORS.len(),
            TargetName::ALL.len(),
            "every target's entry minor is recorded exactly once"
        );
        for &(target, minor) in TARGET_ENTRY_MINORS {
            assert!(
                TargetName::ALL.contains(&target),
                "`{}` has a recorded minor but is not in the vocabulary",
                target.as_str()
            );
            assert_eq!(
                target.abi_minor(),
                minor,
                "`{}` reports a different minor than the one recorded",
                target.as_str()
            );
        }
        for target in TargetName::ALL {
            assert!(
                TARGET_ENTRY_MINORS.iter().any(|&(t, _)| t == target),
                "`{}` entered the vocabulary with no recorded minor",
                target.as_str()
            );
        }
    }

    /// A reserved name is one the vocabulary does *not* hold. The moment a name
    /// becomes requestable its reserved entry is dead — [`resolve_target`]
    /// consults the vocabulary first — and a dead entry is a diagnostic nobody
    /// can reach and nobody notices is wrong.
    #[test]
    fn no_reserved_name_is_also_requestable() {
        for reserved in RESERVED_TARGET_NAMES {
            assert_eq!(
                TargetName::from_name(reserved),
                None,
                "`{reserved}` is requestable, so remove it from `RESERVED_TARGET_NAMES`"
            );
        }
    }

    /// Driven from the vocabulary rather than from one line per name: the loop
    /// over the two surfaces already scaled and the assertions inside it did
    /// not, so a name added to `ALL` used to get no per-surface coverage at all.
    #[test]
    fn a_supported_name_resolves_on_either_surface() {
        for source in [TargetSource::Manifest, TargetSource::Flag] {
            for target in TargetName::ALL {
                assert_eq!(resolve_target(target.as_str(), source), Ok(target));
            }
        }
    }

    /// The predicates a front end holding only a name can answer, pinned per
    /// variant. Two of them are copies of the emission-side target's, and the
    /// mirror test in `inference-wasm-codegen` is what holds the copies to the
    /// originals; this pins what the copies say, so a change here is deliberate
    /// rather than a silent widening of what a manifest may ask for.
    ///
    /// The third has no emission-side original — whether a plain runtime can
    /// invoke the module is not a question code generation answers — so these
    /// assertions are the only thing pinning it at all.
    ///
    /// Every conversion of a `matches!(self, Self::Wasm32)` into an exhaustive
    /// match belongs here in the same change. A new variant inherits `false`
    /// from the concise form and states `false` under the exhaustive one, which
    /// is the same answer: without the pin the conversion has no observable
    /// effect and reads as a no-op refactor.
    #[test]
    fn each_target_records_what_it_permits() {
        assert!(TargetName::Wasm32.supports_proof_mode());
        assert!(TargetName::Wasm32.permits_bulk_memory());
        assert!(TargetName::Wasm32.runs_under_a_plain_wasm_runtime());
        assert!(!TargetName::Stellar.supports_proof_mode());
        assert!(!TargetName::Stellar.permits_bulk_memory());
        assert!(!TargetName::Stellar.runs_under_a_plain_wasm_runtime());
        assert!(!TargetName::SpaceWasm.supports_proof_mode());
        assert!(!TargetName::SpaceWasm.permits_bulk_memory());
        assert!(TargetName::SpaceWasm.runs_under_a_plain_wasm_runtime());
    }

    /// The two per-name clauses, pinned the same way and for the same reason:
    /// each is an exhaustive match whose `None` arm is a decision.
    ///
    /// The pairs asserted are what makes a decided `None` distinguishable from
    /// an unwritten one. `Wasm32` is never forwarded and supports proof mode, so
    /// both of its clauses are `None` by construction; every other name has to
    /// have decided at least the first, because a non-default name is exactly
    /// what can be dropped on the way to an older compiler.
    ///
    /// What each clause *says* is pinned too, and not only that one exists. The
    /// point of a per-name clause is that the names differ, so a clause pasted
    /// from the neighbouring arm is the mistake worth catching, and a
    /// non-emptiness check cannot see it. The phrase a clause is held to is
    /// chosen in an exhaustive match on the name, so a name added to the
    /// vocabulary is pinned to saying something of its own rather than counted.
    ///
    /// Fails if a name is added with no consequence, if one arm's text is copied
    /// into the other, if either clause is blank — each is appended verbatim to a
    /// refusal, so a blank one is a trailing space and no sentence — if either
    /// starts naming a selection surface (the two that exist spell a target
    /// differently and these clauses are appended on both), or if Stellar's proof
    /// refusal acquires a per-name clause that is not true of it.
    #[test]
    fn each_non_default_target_states_what_dropping_it_would_cost() {
        assert_eq!(TargetName::Wasm32.unforwarded_consequence(), None);
        assert_eq!(TargetName::Wasm32.proof_refusal_remediation(), None);

        for target in TargetName::ALL {
            if target == TargetName::DEFAULT {
                continue;
            }
            let consequence = target.unforwarded_consequence().unwrap_or_else(|| {
                panic!(
                    "`{}` can be dropped on the way to an older compiler and says \
                     nothing about what that would cost",
                    target.as_str()
                )
            });
            let distinctive = match target {
                TargetName::Wasm32 => unreachable!("the default is skipped above"),
                TargetName::Stellar => "would not be a contract",
                TargetName::SpaceWasm => "the same bytes",
            };
            assert!(
                consequence.contains(distinctive),
                "`{}`'s clause must say what dropping this name costs, in the terms \
                 only it can be described in: {consequence}",
                target.as_str()
            );

            for clause in [Some(consequence), target.proof_refusal_remediation()]
                .into_iter()
                .flatten()
            {
                assert!(
                    !clause.trim().is_empty(),
                    "`{}` carries a clause appended verbatim to a refusal, so a \
                     blank one is a trailing space and no sentence",
                    target.as_str()
                );
                assert!(
                    !clause.contains("--target") && !clause.contains("[build] target"),
                    "`{}`'s clauses are appended to messages on both selection \
                     surfaces, so neither may name one: {clause}",
                    target.as_str()
                );
            }
        }

        assert_ne!(
            TargetName::Stellar.unforwarded_consequence(),
            TargetName::SpaceWasm.unforwarded_consequence(),
            "two names whose artifacts differ in kind cannot share one clause"
        );

        assert_eq!(
            TargetName::Stellar.proof_refusal_remediation(),
            None,
            "a target whose artifact is a rewrite of the default's takes the \
             book's procedure, not an appended clause"
        );
        assert!(
            TargetName::SpaceWasm
                .proof_refusal_remediation()
                .is_some_and(|clause| clause.contains("Prove the program in a `wasm32` build")),
            "a target whose artifact is the default's must name the build that \
             does produce a proof"
        );
    }

    #[test]
    fn target_source_selects_the_spelling_the_message_names() {
        assert_eq!(TargetSource::Manifest.label(), "`[build] target`");
        assert_eq!(TargetSource::Flag.label(), "`--target`");
    }

    #[test]
    fn the_supported_listing_is_rendered_from_the_vocabulary() {
        assert_eq!(supported_targets_listing(), "`wasm32`, `stellar`, `spacewasm`");
    }

    /// The rendering is pinned character for character: the `#[error(...)]`
    /// attribute is the only copy of this wording and both front ends show it
    /// verbatim.
    #[test]
    fn an_unknown_target_lists_the_supported_set() {
        let err = resolve_target("wasm64", TargetSource::Manifest)
            .expect_err("`wasm64` is not in the vocabulary");
        assert_eq!(
            err.to_string(),
            "Invalid `[build] target` value `wasm64`: unknown compilation target. \
             Supported targets: `wasm32`, `stellar`, `spacewasm`."
        );
    }

    /// The former name earns the dedicated sentence rather than the generic
    /// "unknown target", which would be misleading about a runtime the compiler
    /// does build for — and the sentence names the spelling that works, so the
    /// message is a redirect rather than a refusal. Pinned character for
    /// character: the `#[error(...)]` attribute is the only copy of this
    /// wording.
    #[test]
    fn the_reserved_target_error_renders_its_exact_wording() {
        assert_eq!(
            resolve_target("soroban", TargetSource::Flag)
                .expect_err("`soroban` is the former name")
                .to_string(),
            "Invalid `--target` value `soroban`: `soroban` is the former name of the `stellar` \
             target and is not accepted; write `stellar` instead. Supported targets: `wasm32`, \
             `stellar`, `spacewasm`."
        );
    }

    /// The redirect must name a spelling that resolves, or it sends the user
    /// from one rejection to another. Nothing else ties the sentence to the
    /// vocabulary: the name is written into the `#[error(...)]` attribute by
    /// hand.
    #[test]
    fn the_redirect_names_a_target_that_resolves() {
        let message = resolve_target("soroban", TargetSource::Manifest)
            .expect_err("`soroban` is the former name")
            .to_string();
        let suggested = message
            .split("write `")
            .nth(1)
            .and_then(|rest| rest.split('`').next())
            .expect("the redirect names a spelling to write");
        assert_eq!(
            resolve_target(suggested, TargetSource::Manifest),
            Ok(TargetName::Stellar),
            "the redirect suggests `{suggested}`, which does not resolve"
        );
    }

    /// A trailing space inside a TOML string is invisible in the echoed entry, so
    /// the message has to name the cause or the user reads their own spelling
    /// reported back as unknown.
    #[test]
    fn whitespace_around_a_supported_target_is_named_as_the_cause() {
        let err = resolve_target(" wasm32", TargetSource::Flag)
            .expect_err("whitespace is rejected, never trimmed");
        assert_eq!(
            err.to_string(),
            "Invalid `--target` value ` wasm32`: unknown compilation target. Supported targets: \
             `wasm32`, `stellar`, `spacewasm`. Target names are matched exactly and this entry \
             has surrounding whitespace: write `wasm32`."
        );
    }

    /// The hint reaches the reserved names too: without it `"soroban "` reads as
    /// an unknown target, which is the message the reserved sentence exists to
    /// replace, with no sign of the space that caused it.
    #[test]
    fn whitespace_around_a_reserved_target_is_named_as_the_cause() {
        let err = resolve_target("soroban ", TargetSource::Manifest)
            .expect_err("whitespace is rejected, never trimmed");
        assert!(
            err.to_string().ends_with(
                "Target names are matched exactly and this entry has surrounding whitespace: \
                 write `soroban`."
            ),
            "got: {err}"
        );
    }

    /// A name with no whitespace and no near-miss earns no hint, so the sentence
    /// stays a signal rather than boilerplate on every rejection.
    #[test]
    fn an_ordinary_typo_earns_no_whitespace_hint() {
        let err = resolve_target("nope", TargetSource::Flag).expect_err("`nope` is unknown");
        assert!(
            !err.to_string().contains("surrounding whitespace"),
            "got: {err}"
        );
    }
}
