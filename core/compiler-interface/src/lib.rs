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
//! [`MemoryLayout`] is the linear memory a build asks for — the page count, the
//! most pages it may grow to, the shadow stack's share of it, and whether the
//! module defines that memory or imports it from the host — together with the
//! invariants that make those numbers a memory a module can actually declare.
//! [`MemoryRequest`] is the same request before any number is filled or
//! checked, and [`MemoryImport`] is the one spelling of the import an imported
//! layout asks its host for. The vocabulary lives here for the same reason the
//! feature vocabulary does: the surfaces that select a layout and the code
//! generation that emits one must agree on which layouts exist, and a rejection
//! has to read the same whether the numbers came from a manifest or a command
//! line.
//!
//! [`MemoryLayout::resolve`] is the only way to name a layout other than the
//! default, so the invariants hold by construction rather than by a check every
//! consumer has to remember to run.
//!
//! # Host-import vocabulary
//!
//! [`HOST_SEGMENT`] is the module-path segment that names the embedder rather
//! than a linked module. `infc` classifies a `use … from` clause by it and
//! `infs` refuses an `Inference.toml [wasm-dependencies]` key under it, so the
//! two must spell it alike.
//!
//! # Bounds-check vocabulary
//!
//! [`BoundsChecks`] is the policy deciding which array accesses keep their
//! runtime bounds guard: every one ([`BoundsChecks::All`], the default), or only
//! those the analysis did not prove in bounds ([`BoundsChecks::OmitProven`]).
//! [`resolve_bounds_checks`] is the shared validation, so `infs` (reading
//! `Inference.toml`) and `infc` (reading `--bounds-checks`) reject the same
//! spellings with the same words.

pub mod errors;

pub use crate::errors::{
    BoundsChecksError, MemoryLayoutError, StaticDataError, TargetError, WasmFeatureError,
};

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
///
/// Minor 9 adds the additive `--bounds-checks <all|omit-proven>` flag to
/// `infc`, choosing whether an array access the analysis proved in bounds keeps
/// its runtime guard ([`BoundsChecks`]). It is backward compatible in the same
/// sense as the minors above: omitting the flag selects `all`, which keeps every
/// guard, exactly as every earlier minor did, so a minor-8 `infs` still pairs
/// with a minor-9 `infc` and loses nothing by it — a minor-8 `infs` predates the
/// `Inference.toml [build] bounds-checks` key, so it holds no policy to drop.
/// The pairing callers must gate on is an `infs` holding `omit-proven` talking
/// to a minor-8 `infc`, and it is the first flag whose dropped forward errs on
/// the side of safety: the build would succeed and ship every guard the project
/// asked to omit, a larger and slower module rather than a less safe one. The
/// gate is on this constant all the same, because that module is still not the
/// one the manifest describes — its size, its SpaceWasm envelope and its record
/// of omitted guards are all the guarded build's — and a manifest key that
/// silently does nothing is what every gate here exists to rule out.
///
/// Minor 10 adds the additive `--max-memory-pages <N>` flag to `infc`, raising
/// the linear memory's maximum above its size so a linked module or the host can
/// grow it ([`MemoryLayout::max_pages`]). It is backward compatible in the same
/// sense as the minors above: omitting the flag makes the maximum the size, the
/// fixed memory every earlier minor emitted, so a minor-9 `infs` still pairs
/// with a minor-10 `infc` and loses nothing by it — a minor-9 `infs` predates the
/// `Inference.toml [memory] max-pages` key, so it holds no maximum to drop. The
/// pairing callers must gate on is an `infs` holding a maximum talking to a
/// minor-9 `infc`. A forward that reaches one fails loudly on a flag it does not
/// parse; a caller that decided the flag was unnecessary and omitted it would
/// build a fixed memory instead, and the difference surfaces only later and
/// only sometimes — a linked module that grows memory is refused at the link,
/// but a host that grows the exported memory at run time is simply told the
/// growth failed. So the gate is on this constant, as it is for every flag
/// minor, and it is on this minor alone: a project that sets only `pages` or
/// `stack-size` still needs no more than minor 3.
///
/// Minor 11 adds the additive `--import-memory` flag to `infc`, having the
/// emitted module import its linear memory from the host as `env.memory`
/// rather than define it ([`MemoryLayout::is_imported`], [`MemoryImport`]). It
/// is backward compatible in the same sense as the minors above: omitting the
/// flag defines the memory, as every earlier minor did, so a minor-10 `infs`
/// still pairs with a minor-11 `infc` and loses nothing by it — a minor-10
/// `infs` predates the `Inference.toml [memory] import` key, so it holds no
/// import to drop. The pairing callers must gate on is an `infs` holding
/// `import = true` talking to a minor-10 `infc`. A forward that reaches one
/// fails loudly on a flag it does not parse; a dropped forward is the first of
/// the eleven whose module runs against the host it was built for while
/// computing on a different memory. The build succeeds with a module that
/// defines its own memory, and an embedder that resolves imports by name, as
/// the JavaScript API and Wasmtime's `Linker` do, ignores the `env.memory` it
/// provides and the module does not ask for, so instantiation succeeds too;
/// the host then reads and writes a memory the module never touches. Nothing
/// fails: the two simply disagree. The bytes do record it, as a memory section
/// in place of the import, but only for a reader who knows to look. So the
/// gate is on this constant, and on this minor alone: `import = false`
/// forwards nothing and needs no floor.
pub const COMPILER_ABI_MINOR: u32 = 11;

/// The first segment of a `use … from` clause that names the embedder rather
/// than a linked module: `use { clock_ms } from host::env;` binds a host import
/// of the module `env`.
///
/// `infs` reads this copy to refuse a `[wasm-dependencies]` key the compiler
/// would refuse as a `--wasm-dep`. The compiler's own is
/// `inference_type_checker::HOST_SEGMENT`, a separate constant because the type
/// checker does not depend on this crate; `infc`'s
/// `the_host_segment_infs_refuses_is_the_one_the_compiler_reserves` holds the
/// two equal.
pub const HOST_SEGMENT: &str = "host";

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
    /// instruction family, no imported memory, and an exported function
    /// carrying only the scalar types the convention can encode without a host
    /// object. Code generation owns the last of those; the rest are the ones a
    /// front end can answer before spawning a compiler at all.
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
    /// not define the custom `0xfc` instructions; no post-MVP instruction
    /// family, because it implements none of the proposals that add one; no
    /// memory growth, because the interpreter is loaded with `memory.grow`
    /// disallowed; and, for now, no imported memory (#547). The decode-time
    /// maxima the interpreter also enforces — parameter and local word counts,
    /// name lengths, host arity — are properties of the finished module rather
    /// than of the request, so nothing here can answer them.
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

    /// Whether a build for this target may import its linear memory from the
    /// host rather than define it.
    ///
    /// The same mirror rule as [`Self::supports_proof_mode`]: the emission-side
    /// target decides, this copy exists for a front end that has only the name.
    ///
    /// Named `supports_*` after `Target::supports_host_imports` in
    /// `inference-wasm-codegen`: like that predicate, and unlike the
    /// `permits_*` ones, it is not a claim about what the decoder reads.
    /// `Stellar` answers `false` because the value-ABI rewrite accepts no
    /// import, and a contract that has a memory defines and exports it.
    /// `SpaceWasm` answers `false` for now: the interpreter decodes a memory
    /// import and can bind one its embedder registers, but this toolchain does
    /// not build for such an embedder and the F´ reference hosts register
    /// functions only. Issue #547 tracks lifting it.
    #[must_use]
    pub fn supports_imported_memory(self) -> bool {
        match self {
            Self::Wasm32 => true,
            Self::Stellar | Self::SpaceWasm => false,
        }
    }

    /// Whether this target's runtime can grow a linear memory, so a layout
    /// whose maximum exceeds its size declares room that can actually be
    /// reached.
    ///
    /// The same mirror rule as [`Self::supports_proof_mode`]: the emission-side
    /// target decides, this copy exists for a front end that has only the name.
    ///
    /// `SpaceWasm` answers `false` because its interpreter is loaded with
    /// `memory.grow` disallowed, so a maximum above the size would promise
    /// room the module can never have; the engines behind `Wasm32` and
    /// `Stellar` grow a memory up to its declared maximum.
    #[must_use]
    pub fn permits_memory_growth(self) -> bool {
        match self {
            Self::Wasm32 | Self::Stellar => true,
            Self::SpaceWasm => false,
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

/// Which array accesses keep their runtime bounds guard, as a project's
/// `Inference.toml` and the `infc --bounds-checks` command line name the policy.
///
/// Every array access whose index is not a bare number literal is emitted with a
/// guard that traps on an out-of-range index, and rule A056 separately requires
/// the analysis to prove each such index in bounds before the program compiles
/// at all. The two are independent layers: under [`Self::All`], the default, an
/// access is safe if the analysis is right *or* the guard runs, and the guard
/// always runs. [`Self::OmitProven`] drops the guard wherever the analysis
/// proved it can never fire, trading that second layer for size and speed. At
/// those accesses safety then rests on the range analysis alone, a flaw in it
/// reads or writes the neighbouring linear memory where it would have trapped,
/// and a Rocq proof about the module no longer re-establishes the bound itself.
///
/// The policy describes the artifact, not how it was built, so it applies alike
/// in compile and proof mode and at every target: nothing that acts on it may
/// consult either, or a proof would describe a module other than the one that
/// ships. An artifact built under [`Self::OmitProven`] records the functions it
/// omitted a guard in, in its `inference.bounds_elided` custom section.
///
/// The accessor below is a match rather than `matches!`, for the reason
/// [`TargetName`] gives: a new variant must be decided, not handed a default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BoundsChecks {
    /// Every access keeps its guard, proven or not. The default, and what every
    /// build emitted before the policy could be chosen.
    #[default]
    All,
    /// An access the analysis proved in bounds is emitted without its guard;
    /// every other access keeps it.
    OmitProven,
}

impl BoundsChecks {
    /// The policy a build gets when it names none.
    pub const DEFAULT: Self = Self::All;

    /// Every selectable policy, in canonical order. The rendered supported-set
    /// listing in diagnostics comes from here.
    pub const ALL: [Self; 2] = [Self::All, Self::OmitProven];

    /// The policy as it is written in `Inference.toml` and on the
    /// `--bounds-checks` command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::OmitProven => "omit-proven",
        }
    }

    /// The policy written exactly as `name`, or `None`.
    ///
    /// The inverse of [`Self::as_str`]: matching is exact and case-sensitive,
    /// and no whitespace is trimmed, for the reason
    /// [`WasmFeatureName::from_name`] gives.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == name)
    }

    /// Whether an access the analysis proved in bounds is emitted without its
    /// guard.
    #[must_use]
    pub fn omits_proven(self) -> bool {
        match self {
            Self::All => false,
            Self::OmitProven => true,
        }
    }
}

/// Which surface a bounds-check policy was named on, so a diagnostic can name
/// the exact thing the user has to edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundsChecksSource {
    /// The `bounds-checks` key of a project's `Inference.toml` `[build]` table.
    Manifest,
    /// The `--bounds-checks` flag on an `infc` command line.
    Flag,
}

impl BoundsChecksSource {
    /// The backtick-quoted surface name a message points the user at.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manifest => "`[build] bounds-checks`",
            Self::Flag => "`--bounds-checks`",
        }
    }
}

/// Resolves the bounds-check policy named on `source`, or returns the
/// diagnostic that rejects it.
///
/// This is the whole validation both front ends run. Matching is exact and
/// case-sensitive and no whitespace is trimmed, for the reason
/// [`BoundsChecks::from_name`] gives.
///
/// # Errors
///
/// Returns [`BoundsChecksError`] for any name outside the vocabulary.
pub fn resolve_bounds_checks(
    entry: &str,
    source: BoundsChecksSource,
) -> Result<BoundsChecks, BoundsChecksError> {
    BoundsChecks::from_name(entry).ok_or_else(|| BoundsChecksError {
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
/// Every key of a surface is named together because [`MemoryLayout::resolve`]
/// checks the numbers jointly: several invariants — a stack that outgrows its
/// memory, a maximum below the size it caps, a memory that leaves the overflow
/// trap no room — are properties of a pair, and attributing those to one key
/// would name the wrong one half the time. The `reason` on
/// [`MemoryLayoutError`] identifies the offending value; this identifies where
/// it was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayoutSource {
    /// The `pages` / `max-pages` / `stack-size` keys of a project's
    /// `Inference.toml` `[memory]` table.
    Manifest,
    /// The `--memory-pages` / `--max-memory-pages` / `--stack-size` flags on an
    /// `infc` command line.
    Flag,
}

impl MemoryLayoutSource {
    /// The backtick-quoted keys a message points the user at.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Manifest => "`[memory] pages` / `[memory] max-pages` / `[memory] stack-size`",
            Self::Flag => "`--memory-pages` / `--max-memory-pages` / `--stack-size`",
        }
    }
}

/// A memory layout as a build asked for it: every dimension a request may leave
/// unset, before [`MemoryLayout::resolve`] fills and checks them.
///
/// A struct rather than positional arguments to the constructor: the three
/// numbers are all `Option<u32>`, and a call that transposed two of them would
/// compile and resolve a different memory, while a bare `true` among them would
/// not say what it turns on. Naming each field at the call site is what keeps a
/// stack size from being read as a page count, and an import from being read
/// as anything else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryRequest {
    /// Linear memory size in 64 KiB pages. Unset means one page.
    pub pages: Option<u32>,
    /// The most pages the memory may grow to. Unset means the memory's own
    /// size — whatever `pages` resolves to — so the memory is fixed.
    pub max_pages: Option<u32>,
    /// Shadow stack size in bytes. Unset means 64 KiB, less whatever part of
    /// it the program's static data needs when the memory has no other room
    /// for it (see [`MemoryLayout::with_static_data`]).
    pub stack_size: Option<u32>,
    /// Import the memory from the host as `env.memory` instead of defining it.
    /// Unset (false) defines it; in a change list from
    /// [`MemoryLayout::request_to_fit`] it means the import is unchanged.
    pub import: bool,
}

/// The linear memory a generated module declares, how far it may grow, and the
/// share of it the shadow stack occupies.
///
/// This is shared vocabulary rather than a code-generation detail: a project's
/// manifest, the compiler flags that override it, and the emitter that turns the
/// three numbers into a memory section, or a memory import, and a
/// `__stack_pointer` initializer all read this one type. A layout a build
/// accepts is therefore exactly a layout that can be emitted.
///
/// It is deliberately not mirrored by a code-generation twin the way
/// [`WasmFeatureName`] is by `EmitFeatures`. That mirror earns its place because
/// a requested proposal and an emission permission are genuinely different
/// things: the set a user may ask for is not the set an emitter knows how to
/// produce. A layout has no such second reading — the pages and stack bytes a
/// user writes are the pages and stack bytes emitted — so a mirror would buy
/// nothing but a second place for the invariants below to drift apart.
///
/// The memory is fixed unless a build asks otherwise: the maximum defaults to
/// the size, so the memory section (or the memory import) declares
/// `min == max` and `memory.grow` can never move the end of memory. A build
/// may raise the maximum above the size, which lets a linked module or the
/// host grow the memory up to it; nothing this compiler emits grows memory
/// itself. The size is what the stack must fit in, since it is all a module is
/// guaranteed at instantiation, and the maximum is what the overflow trap's
/// headroom is measured against, since it is how far growth can move the end
/// of memory.
///
/// Code generation places the shadow stack at the bottom of memory: it spans
/// `[0, stack_size)` and `__stack_pointer` grows downward from `stack_size`
/// toward 0. Directly above it, from `stack_size`, lies the **static data
/// region**: the bytes of the program's array and struct module constants,
/// which one active data segment writes at instantiation and which nothing the
/// compiler emits ever writes again. Whatever lies above the region, up to
/// `pages * 64 KiB`, is ordinary addressable memory nothing the compiler emits
/// touches. None of it is a hole: an access that strays there succeeds rather
/// than trapping, so a stack larger than the program needs is not free (see
/// `core/wasm-linker`, which leans on an out-of-region address usually being out
/// of bounds).
///
/// A build may ask for the memory to be imported from the host as `env.memory`
/// rather than defined by the module. The import declares the same limits a
/// defined memory would — `pages` as its minimum, `max_pages` as its maximum —
/// and the stack and the static data are placed in it exactly as they are in a
/// defined one, so every invariant [`Self::resolve`] checks holds unchanged.
/// The imported memory is memory 0, the one every load and store addresses.
/// [`Self::memory_import`] is what such a layout asks its host for.
///
/// A layout [`Self::resolve`] returns places no data yet: how much the program
/// needs is known only once it has been type-checked.
/// [`Self::with_static_data`] places it, and is the one place the stack and the
/// data are judged together — the program's whole static footprint, which must
/// fit the pages the module is guaranteed at instantiation.
///
/// The three numbers form one type because none is checkable alone: a stack
/// size is only sane relative to the memory it must fit in, a page count is only
/// sane relative to the stack it must hold, a maximum only relative to the size
/// it caps, and the overflow trap needs the maximum and the stack together to
/// leave headroom below 2^32. [`Self::resolve`] is where that joint contract
/// lives, and it is the only way to name a layout other than [`Self::default`].
/// The fields are private so that holding a value of this type *is* the
/// guarantee that the contract holds — a consumer reads the numbers through
/// [`Self::pages()`], [`Self::max_pages()`] and [`Self::stack_size()`] without
/// owing anyone a validation step, and no caller can assemble a memory the
/// emitter would have to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryLayout {
    /// Linear memory size in 64 KiB pages: the memory section's minimum, or
    /// the memory import's, and every page a module is guaranteed at
    /// instantiation.
    pages: u32,
    /// The most pages the memory may grow to: the memory section's maximum,
    /// or the memory import's. Equal to `pages` unless the build asked for
    /// more, so by default the memory is fixed rather than growable.
    max_pages: u32,
    /// Size of the shadow-stack region in bytes, occupying `[0, stack_size)`.
    stack_size: u32,
    /// Whether the build asked for `stack_size`. A requested stack is kept as
    /// asked, and static data has to fit beside it; a default one gives up the
    /// part of its 64 KiB the data needs when the memory has no other room.
    stack_requested: bool,
    /// Bytes of static data placed at `[stack_size, stack_size + data_size)`.
    /// Zero until [`Self::with_static_data`] places some.
    data_size: u32,
    /// Whether the host supplies the memory, imported as `env.memory` with
    /// these limits, rather than the module defining it. Changes where the
    /// memory comes from, never its shape.
    import: bool,
}

/// Implemented by hand rather than derived: a derived `Default` would produce a
/// zero-page, zero-byte memory, which is not a layout any program can run in.
/// These are instead exactly the values every build emitted before the layout
/// became configurable — one fixed page, entirely stack — so a default build's
/// bytes are unchanged.
impl Default for MemoryLayout {
    fn default() -> Self {
        Self {
            pages: 1,
            max_pages: 1,
            stack_size: DEFAULT_STACK_SIZE,
            stack_requested: false,
            data_size: 0,
            import: false,
        }
    }
}

/// The shadow stack a build gets when it asks for no size: one page.
const DEFAULT_STACK_SIZE: u32 = PAGE_SIZE;

/// The largest linear memory a 32-bit WebAssembly module may declare: 65536
/// pages of 64 KiB each is the whole 4 GiB address space.
const MAX_PAGES: u32 = 65_536;

/// The 32-bit address space in bytes.
///
/// The stack-overflow trap depends on a wrapped frame pointer landing past the
/// end of memory, so the memory at its maximum and the stack must fit inside
/// this together — see the headroom invariant enforced by
/// [`MemoryLayout::resolve`]. That is a stricter bound than [`MAX_PAGES`] alone,
/// and it is why a module may not declare the whole address space.
const ADDRESS_SPACE: u64 = 1 << 32;

impl MemoryLayout {
    /// The layout a build asked for, with every dimension the request left unset
    /// taken from [`Self::default`] — except the maximum, which is taken from
    /// the resolved size.
    ///
    /// This is the checked constructor: partial specification is meaningful — a
    /// project that sets only `pages` wants the default stack inside a larger
    /// memory — so the filling happens before the check rather than after, and a
    /// request is judged as the whole layout it produces. That ordering is what
    /// lets a single well-formed number still be rejected: a 128 KiB stack is
    /// fine on its own terms and impossible inside the default one page.
    ///
    /// An unset maximum is the resolved size rather than the default one, so a
    /// request for four pages and no maximum is a fixed four-page memory — not a
    /// four-page memory capped at one, which no module could declare.
    ///
    /// `surface` selects only the spelling a rejection names. The same numbers
    /// are accepted or refused identically whether they came from a manifest or
    /// a command line, which is the property that makes this the one definition
    /// of a legal layout.
    ///
    /// # Errors
    ///
    /// Returns the violated invariant, rendered against `surface`.
    pub fn resolve(
        request: MemoryRequest,
        surface: MemoryLayoutSource,
    ) -> Result<Self, MemoryLayoutError> {
        let MemoryRequest {
            pages,
            max_pages,
            stack_size,
            import,
        } = request;
        let defaults = Self::default();
        let pages = pages.unwrap_or(defaults.pages);
        let layout = Self {
            pages,
            max_pages: max_pages.unwrap_or(pages),
            stack_size: stack_size.unwrap_or(defaults.stack_size),
            stack_requested: stack_size.is_some(),
            data_size: 0,
            import,
        };
        layout
            .validate()
            .map_err(|reason| MemoryLayoutError { reason, surface })?;
        Ok(layout)
    }

    /// Linear memory size in 64 KiB pages: the memory section's minimum, or
    /// the memory import's.
    #[must_use]
    pub fn pages(self) -> u32 {
        self.pages
    }

    /// The most pages the memory may grow to: the memory section's maximum,
    /// or the memory import's. Equal to [`Self::pages()`] unless the build
    /// asked for more.
    #[must_use]
    pub fn max_pages(self) -> u32 {
        self.max_pages
    }

    /// Whether the memory may grow: its maximum exceeds its size.
    ///
    /// False for every layout a build did not explicitly ask to be growable,
    /// the default included.
    #[must_use]
    pub fn is_growable(self) -> bool {
        self.max_pages > self.pages
    }

    /// Size of the shadow-stack region in bytes, occupying `[0, stack_size)`.
    #[must_use]
    pub fn stack_size(self) -> u32 {
        self.stack_size
    }

    /// Whether the build asked for the stack size, rather than taking the
    /// default. Only a default stack gives way to static data.
    #[must_use]
    pub fn is_stack_requested(self) -> bool {
        self.stack_requested
    }

    /// Bytes of static data the layout places above the stack. Zero for a
    /// layout no data has been placed in.
    #[must_use]
    pub fn data_size(self) -> u32 {
        self.data_size
    }

    /// The address the static data region starts at: the top of the stack.
    #[must_use]
    pub fn data_base(self) -> u32 {
        self.stack_size
    }

    /// Whether the host supplies the memory rather than the module defining
    /// it.
    ///
    /// False for every layout a build did not explicitly ask to import, the
    /// default included.
    #[must_use]
    pub fn is_imported(self) -> bool {
        self.import
    }

    /// The memory import this layout asks its host for, or `None` when the
    /// module defines its memory.
    ///
    /// The limits are the layout's own: the size as the minimum and the
    /// maximum as the maximum, the same pair a defined memory would declare.
    #[must_use]
    pub fn memory_import(self) -> Option<MemoryImport> {
        self.import.then_some(MemoryImport {
            minimum: self.pages,
            maximum: self.max_pages,
        })
    }

    /// Whether the layout describes exactly the memory a build that configures
    /// nothing gets: one fixed page, all of it stack, no data, defined by the
    /// module.
    ///
    /// Compares the memory, not how it was asked for, so `--stack-size 65536`
    /// written out is the default memory too. Code generation reads this to
    /// decide whether a program that touches no memory still has to declare
    /// one.
    ///
    /// An imported memory is never the default, whatever its numbers: a
    /// program that touches no memory still imports one, because the contract
    /// an embedder is written to — supply `env.memory` — must not depend on
    /// whether the program happens to touch memory. Otherwise the first edit
    /// that did would silently change what the host has to provide.
    #[must_use]
    pub fn is_default_memory(self) -> bool {
        let default = Self::default();
        self.pages == default.pages
            && self.max_pages == default.max_pages
            && self.stack_size == default.stack_size
            && self.data_size == 0
            && !self.import
    }

    /// The layout with `data_bytes` of static data placed directly above the
    /// stack: the program's whole static footprint, judged against the memory.
    ///
    /// A requested stack is kept, and the data has to fit between its top and
    /// the end of the `pages` the module is guaranteed. A default stack keeps
    /// its 64 KiB when the memory has room for the data above it and otherwise
    /// gives up exactly the part the data needs, rounded to the frame grid —
    /// so a one-page program with 2 KiB of constant tables gets a 62 KiB stack
    /// rather than a build error, and analysis measures its call chains
    /// against that. The data is measured against `pages`, not `max_pages`:
    /// it is written at instantiation, before any growth.
    ///
    /// Placing is idempotent from the build's request: a layout that already
    /// holds data is re-placed as if it held none, so a caller holding either
    /// gets the same answer.
    ///
    /// # Errors
    ///
    /// Returns the footprint that does not fit: a requested stack and the data
    /// that need more than the memory, or data that leaves a default stack less
    /// than one frame.
    pub fn with_static_data(self, data_bytes: u64) -> Result<Self, StaticDataError> {
        let memory_bytes = u64::from(self.pages) * u64::from(PAGE_SIZE);
        let refused = |stack_bytes: u32| StaticDataError {
            data_bytes,
            stack_bytes,
            stack_requested: self.stack_requested,
            pages: self.pages,
        };
        let stack_size = if self.stack_requested {
            if u64::from(self.stack_size) + data_bytes > memory_bytes {
                return Err(refused(self.stack_size));
            }
            self.stack_size
        } else {
            let room = memory_bytes.saturating_sub(data_bytes);
            let room = room - room % u64::from(FRAME_ALIGNMENT);
            if room < u64::from(FRAME_ALIGNMENT) {
                return Err(refused(FRAME_ALIGNMENT));
            }
            // `room` is below the memory size, and so below 2^32 for every
            // layout `resolve` accepts.
            u32::try_from(room.min(u64::from(DEFAULT_STACK_SIZE))).unwrap_or(DEFAULT_STACK_SIZE)
        };
        let data_size = u32::try_from(data_bytes).map_err(|_| refused(stack_size))?;
        Ok(Self {
            stack_size,
            data_size,
            ..self
        })
    }

    /// The smallest change to the build's request that gives the stack at
    /// least `stack_bytes` and still holds `data_bytes` of static data above it,
    /// or `None` when no layout can.
    ///
    /// Only what has to change is set: the page count when the memory has to
    /// grow, the stack size when a default stack would not be enough or a
    /// requested one is too small, and the maximum when a growable memory's cap
    /// is below the new size. A fixed memory stays fixed. The import is left
    /// unset even for an imported layout: no fit needs the memory to come from
    /// somewhere else, so it is never part of the change. The answer is checked
    /// by [`Self::resolve`] and [`Self::with_static_data`] themselves, on the
    /// layout the build would have after following it, import included, so a
    /// suggestion is never a layout either would refuse.
    #[must_use = "returns the request to suggest without modifying the layout"]
    pub fn request_to_fit(self, stack_bytes: u32, data_bytes: u64) -> Option<MemoryRequest> {
        let grid = u64::from(FRAME_ALIGNMENT);
        let needed = u64::from(stack_bytes).div_ceil(grid) * grid;
        let page = u64::from(PAGE_SIZE);
        let keep_default_stack = !self.stack_requested && needed <= u64::from(DEFAULT_STACK_SIZE);
        let stack = if keep_default_stack {
            needed
        } else {
            needed.max(u64::from(self.stack_size))
        };
        let pages_needed = (stack + data_bytes).div_ceil(page);
        let pages = u32::try_from(pages_needed.max(u64::from(self.pages))).ok()?;
        let request = MemoryRequest {
            pages: (pages > self.pages).then_some(pages),
            max_pages: (self.is_growable() && pages > self.max_pages).then_some(pages),
            stack_size: if keep_default_stack {
                None
            } else {
                let stack = u32::try_from(stack).ok()?;
                (!self.stack_requested || stack > self.stack_size).then_some(stack)
            },
            import: false,
        };
        let effective = MemoryRequest {
            pages: Some(pages),
            max_pages: Some(self.max_pages.max(pages)),
            stack_size: request
                .stack_size
                .or(self.stack_requested.then_some(self.stack_size)),
            // The layout the build would have after following the suggestion,
            // so the check judges the memory the build actually asks for.
            import: self.import,
        };
        let fitted = Self::resolve(effective, MemoryLayoutSource::Flag)
            .ok()?
            .with_static_data(data_bytes)
            .ok()?;
        (u64::from(fitted.stack_size) >= needed).then_some(request)
    }

    /// Checks that the three numbers describe a linear memory a module can
    /// actually declare and code generation can actually address.
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
        // The data region is judged by `with_static_data`, against the stack
        // this check has already accepted; whether the stack was asked for
        // changes no bound here.
        let Self {
            pages,
            max_pages,
            stack_size,
            stack_requested: _,
            data_size: _,
            // An imported memory declares these same limits and import matching
            // caps the host's at the maximum, so every bound holds unchanged.
            import: _,
        } = self;
        let page_size = u64::from(PAGE_SIZE);
        let memory_bytes = u64::from(pages) * page_size;
        let max_memory_bytes = u64::from(max_pages) * page_size;

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
        if max_pages < pages {
            return Err(format!(
                "the linear memory's maximum ({max_pages} pages) is below its size ({pages} \
                 pages); a maximum only admits growth, so it must be at least the size"
            ));
        }
        if max_pages > MAX_PAGES {
            return Err(format!(
                "the linear memory's maximum is limited to {MAX_PAGES} pages (4 GiB) by 32-bit \
                 WebAssembly, but a maximum of {max_pages} pages was requested"
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
        // Measured against the maximum rather than the size: growth moves the
        // end of memory up to it, and a wrapped frame pointer must still land
        // past that end once it has moved.
        let span = max_memory_bytes + u64::from(stack_size);
        if span > ADDRESS_SPACE {
            let (memory, end) = if max_pages == pages {
                (
                    format!("the linear memory ({pages} × 64 KiB = {memory_bytes} bytes)"),
                    "the end of memory",
                )
            } else {
                (
                    format!(
                        "the linear memory at its maximum ({max_pages} × 64 KiB = \
                         {max_memory_bytes} bytes)"
                    ),
                    "the end of memory however far growth moves it",
                )
            };
            return Err(format!(
                "{memory} and the shadow stack ({stack_size} bytes) together span {span} bytes, \
                 more than the {ADDRESS_SPACE}-byte 32-bit address space; a stack overflow wraps \
                 to an address at least {ADDRESS_SPACE} minus the stack size, which must stay \
                 past {end} for the overflow to trap instead of writing into it"
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

/// The memory import an imported layout asks its host for: `env.memory`,
/// with the layout's size as its minimum and its maximum as its maximum.
///
/// Spelled in one place so that code generation, which emits the import from
/// this value, and the post-link check, which compares the finished bytes
/// against it, cannot come to disagree about the name or the limits the way
/// two copies of a string literal could.
///
/// The maximum is always present rather than left to the host. The overflow
/// trap's headroom invariant, `max_pages × 64 KiB + stack_size <= 2^32`, is
/// measured against the maximum, and WebAssembly import matching admits a
/// supplied memory against a declared maximum only if the supplied one has a
/// maximum no greater than it — so a host can never hand over more than
/// `max_pages` pages, and the check [`MemoryLayout::resolve`] made still holds
/// of whatever memory the module runs in. An import with no maximum would
/// admit a memory of any size, and the trap would lose the room it needs. The
/// minimum is the layout's size, which is what the stack and the static data
/// are measured against: it is the least memory a host can supply.
///
/// `env.memory` is the name `wasm-ld --import-memory`, Zig and Emscripten
/// import a memory under by default, so an embedder already supplying memory
/// to one of those needs no new name for this one.
///
/// The fields are public because this is a plain description of an import,
/// with no invariant of its own: the limits hold together only because a
/// [`MemoryLayout`] produced them, and [`MemoryLayout::memory_import`] is the
/// one source a build takes them from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemoryImport {
    /// The fewest 64 KiB pages the supplied memory may have: the layout's
    /// [`MemoryLayout::pages()`].
    pub minimum: u32,
    /// The most 64 KiB pages the supplied memory may have, now or after
    /// growth: the layout's [`MemoryLayout::max_pages()`].
    pub maximum: u32,
}

impl MemoryImport {
    /// The module name the memory is imported from, the first half of the
    /// two-level import name.
    pub const MODULE: &'static str = "env";

    /// The field name the memory is imported as, the second half of the
    /// two-level import name.
    pub const FIELD: &'static str = "memory";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn abi_version_is_one_dot_eleven() {
        assert_eq!(COMPILER_ABI_MAJOR, 1);
        assert_eq!(COMPILER_ABI_MINOR, 11);
    }

    #[test]
    fn every_bounds_check_policy_round_trips_through_its_name() {
        for policy in BoundsChecks::ALL {
            for source in [BoundsChecksSource::Manifest, BoundsChecksSource::Flag] {
                assert_eq!(resolve_bounds_checks(policy.as_str(), source), Ok(policy));
            }
        }
    }

    /// The default keeps every guard, and is the one policy that omits none.
    #[test]
    fn the_default_bounds_check_policy_keeps_every_guard() {
        assert_eq!(BoundsChecks::default(), BoundsChecks::DEFAULT);
        assert_eq!(BoundsChecks::DEFAULT, BoundsChecks::All);
        assert!(!BoundsChecks::All.omits_proven());
        assert!(BoundsChecks::OmitProven.omits_proven());
    }

    #[test]
    fn an_unknown_bounds_check_policy_names_the_surface_and_the_supported_values() {
        let message = resolve_bounds_checks("none", BoundsChecksSource::Manifest)
            .expect_err("`none` is not a policy")
            .to_string();
        assert!(message.contains("`[build] bounds-checks`"), "{message}");
        assert!(message.contains("`none`"), "{message}");
        assert!(message.contains("`all`"), "{message}");
        assert!(message.contains("`omit-proven`"), "{message}");

        let message = resolve_bounds_checks("none", BoundsChecksSource::Flag)
            .expect_err("`none` is not a policy")
            .to_string();
        assert!(message.contains("`--bounds-checks`"), "{message}");
    }

    /// Matching is exact: a case variant and a padded spelling are both refused,
    /// and the padded one is told why.
    #[test]
    fn a_bounds_check_policy_is_matched_exactly() {
        assert!(resolve_bounds_checks("Omit-Proven", BoundsChecksSource::Flag).is_err());
        let message = resolve_bounds_checks("omit-proven ", BoundsChecksSource::Manifest)
            .expect_err("surrounding whitespace is not trimmed")
            .to_string();
        assert!(message.contains("surrounding whitespace"), "{message}");
        assert!(message.contains("write `omit-proven`"), "{message}");
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
        growable(pages, pages, stack_size)
    }

    /// All three dimensions given, the maximum included.
    fn growable(
        pages: u32,
        max_pages: u32,
        stack_size: u32,
    ) -> Result<MemoryLayout, MemoryLayoutError> {
        MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(pages),
                max_pages: Some(max_pages),
                stack_size: Some(stack_size),
                import: false,
            },
            MemoryLayoutSource::Flag,
        )
    }

    /// Only the dimensions a manifest would name, the rest left to the defaults.
    fn requested(
        pages: Option<u32>,
        max_pages: Option<u32>,
        stack_size: Option<u32>,
    ) -> Result<MemoryLayout, MemoryLayoutError> {
        MemoryLayout::resolve(
            MemoryRequest {
                pages,
                max_pages,
                stack_size,
                import: false,
            },
            MemoryLayoutSource::Manifest,
        )
    }

    /// All three dimensions stated, as a request rather than a layout, for the
    /// tables that resolve one request both with and without the import.
    fn stated(pages: u32, max_pages: u32, stack_size: u32) -> MemoryRequest {
        MemoryRequest {
            pages: Some(pages),
            max_pages: Some(max_pages),
            stack_size: Some(stack_size),
            import: false,
        }
    }

    /// `request` with the memory imported from the host instead of defined.
    fn imported(request: MemoryRequest) -> MemoryRequest {
        MemoryRequest {
            import: true,
            ..request
        }
    }

    /// The defined layout with every number of `layout`: what an imported
    /// layout has to equal once its one flag is cleared.
    ///
    /// Reaches past the checked constructor, which is sound here only because
    /// clearing the flag changes nothing [`MemoryLayout::resolve`] judged.
    fn defined_twin(layout: MemoryLayout) -> MemoryLayout {
        MemoryLayout {
            import: false,
            ..layout
        }
    }

    /// Resolves `request` on `surface` once defined and once imported, and
    /// checks the two come out alike: the same refusal, or layouts that differ
    /// in the import alone. Returns the imported verdict for the caller's own
    /// checks.
    fn assert_judged_alike(
        case: &str,
        request: MemoryRequest,
        surface: MemoryLayoutSource,
    ) -> Result<MemoryLayout, MemoryLayoutError> {
        let defined = MemoryLayout::resolve(
            MemoryRequest {
                import: false,
                ..request
            },
            surface,
        );
        let imported = MemoryLayout::resolve(imported(request), surface);
        match (&defined, &imported) {
            (Ok(defined), Ok(imported)) => {
                assert!(
                    imported.is_imported() && !defined.is_imported(),
                    "{case} on {surface:?}: the flag asked for is the flag resolved"
                );
                assert_eq!(
                    defined_twin(*imported),
                    *defined,
                    "{case} on {surface:?}: an imported layout differs from its defined twin in \
                     the import alone"
                );
            }
            (Err(defined), Err(imported)) => assert_eq!(
                imported, defined,
                "{case} on {surface:?}: an imported request must be refused for the reason, and \
                 in the words, its defined twin is"
            ),
            _ => panic!(
                "{case} on {surface:?}: the import changed the verdict: the defined memory \
                 resolved to {defined:?} and the imported one to {imported:?}"
            ),
        }
        imported
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
            MemoryLayout::resolve(MemoryRequest::default(), MemoryLayoutSource::Manifest),
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
        let pages_only = requested(Some(4), None, None).expect("four pages holds the default stack");
        assert_eq!(pages_only.pages(), 4);
        assert_eq!(pages_only.stack_size(), MemoryLayout::default().stack_size());

        let stack_only =
            requested(None, None, Some(32_768)).expect("half a page of stack fits the default page");
        assert_eq!(stack_only.pages(), MemoryLayout::default().pages());
        assert_eq!(stack_only.stack_size(), 32_768);
    }

    /// A default stack keeps its 64 KiB while the memory has room for the data
    /// above it, and gives up exactly what the data needs, on the frame grid,
    /// when it does not. Without the second half a one-page program could not
    /// hold a single constant table without a `[memory]` table.
    #[test]
    fn a_default_stack_gives_way_to_static_data_only_when_the_memory_is_full() {
        let one_page = MemoryLayout::default()
            .with_static_data(2_000)
            .expect("2000 bytes of data fit a page beside a smaller stack");
        // 65536 - 2000 = 63536, already on the 16-byte grid.
        assert_eq!(one_page.stack_size(), 63_536);
        assert_eq!(one_page.data_base(), one_page.stack_size());
        assert_eq!(one_page.data_size(), 2_000);
        assert!(one_page.stack_size() + 2_000 <= PAGE_SIZE);

        let two_pages = requested(Some(2), None, None)
            .expect("two pages")
            .with_static_data(2_000)
            .expect("the data fits above a full default stack");
        assert_eq!(two_pages.stack_size(), 65_536);
        assert_eq!(two_pages.data_base(), 65_536);
    }

    /// A stack the build asked for is never shrunk: the data has to fit beside
    /// it, and the refusal carries the numbers a diagnostic needs.
    #[test]
    fn a_requested_stack_is_kept_and_the_data_must_fit_beside_it() {
        let layout = requested(None, None, Some(65_536)).expect("a full-page stack");
        let err = layout
            .with_static_data(100)
            .expect_err("no room is left above a full-page requested stack");
        assert_eq!(
            err,
            StaticDataError {
                data_bytes: 100,
                stack_bytes: 65_536,
                stack_requested: true,
                pages: 1,
            }
        );
        assert_eq!(err.overflow_bytes(), 100);
        assert!(
            err.to_string()
                .contains("100 more than the 1-page (65536-byte)"),
            "{err}"
        );

        let fits = requested(None, None, Some(32_768))
            .expect("half a page")
            .with_static_data(32_768)
            .expect("the other half holds the data exactly");
        assert_eq!(fits.stack_size(), 32_768);
        assert_eq!(fits.data_size(), 32_768);
    }

    /// Data that would leave a default stack less than one frame is refused
    /// rather than handed a zero-byte stack.
    #[test]
    fn data_that_leaves_no_frame_for_a_default_stack_is_refused() {
        let err = MemoryLayout::default()
            .with_static_data(65_530)
            .expect_err("six bytes are not a frame");
        assert!(!err.stack_requested);
        assert!(
            err.to_string()
                .contains("leave no room for the shadow stack"),
            "{err}"
        );
    }

    /// Placing again re-places from the request, so a caller holding a layout
    /// that already carries data gets the answer one holding none would.
    #[test]
    fn placing_static_data_is_idempotent() {
        let once = MemoryLayout::default()
            .with_static_data(4_096)
            .expect("fits");
        assert_eq!(once.with_static_data(4_096), Ok(once));
        assert_eq!(once.with_static_data(0), Ok(MemoryLayout::default()));
    }

    /// The memory a build that configures nothing gets is recognized by its
    /// numbers, so asking for the default stack size explicitly changes nothing
    /// about what is emitted.
    #[test]
    fn the_default_memory_is_recognized_however_it_was_asked_for() {
        assert!(MemoryLayout::default().is_default_memory());
        let spelled_out = requested(None, None, Some(65_536)).expect("the default stack");
        assert_ne!(
            spelled_out,
            MemoryLayout::default(),
            "the request is remembered"
        );
        assert!(spelled_out.is_default_memory());
        assert!(
            !MemoryLayout::default()
                .with_static_data(16)
                .expect("fits")
                .is_default_memory()
        );
    }

    /// A suggestion changes only what has to change, and is itself a layout the
    /// constructor and the placement accept with room for the stack asked for.
    #[test]
    fn a_request_to_fit_changes_only_what_has_to_change() {
        // The default stack is enough; the memory has to grow for the data.
        assert_eq!(
            MemoryLayout::default().request_to_fit(64_000, 2_048),
            Some(MemoryRequest {
                pages: Some(2),
                max_pages: None,
                stack_size: None,
                import: false
            })
        );
        // More than a default stack: the stack has to be asked for, and the
        // memory grown to hold it.
        assert_eq!(
            MemoryLayout::default().request_to_fit(80_000, 0),
            Some(MemoryRequest {
                pages: Some(2),
                max_pages: None,
                stack_size: Some(80_000),
                import: false
            })
        );
        // A requested stack that is already large enough stays unstated.
        let requested_stack = requested(None, None, Some(32_768)).expect("half a page");
        assert_eq!(
            requested_stack.request_to_fit(16_000, 40_000),
            Some(MemoryRequest {
                pages: Some(2),
                max_pages: None,
                stack_size: None,
                import: false
            })
        );
        // A growable memory whose cap is below the new size raises the cap too.
        let capped = growable(1, 1, 65_536).expect("fixed one page");
        assert_eq!(
            capped.request_to_fit(70_000, 0).map(|r| r.max_pages),
            Some(None),
            "a fixed memory stays fixed: the maximum follows the size"
        );
        let growable_two = growable(1, 2, 65_536).expect("growable to two");
        assert_eq!(
            growable_two
                .request_to_fit(65_536, 70_000)
                .map(|r| (r.pages, r.max_pages)),
            Some((Some(3), Some(3)))
        );
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
        let err = requested(None, None, Some(131_072))
            .expect_err("a 128 KiB stack cannot live in the default single page");
        assert!(
            err.reason.contains("does not fit in the linear memory"),
            "{err}"
        );
        assert!(
            requested(Some(4), None, Some(131_072)).is_ok(),
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

    /// The default memory is fixed: its maximum is its size, so a build that
    /// asks for nothing emits the `min == max` memory every earlier build did.
    #[test]
    fn the_default_layout_is_fixed() {
        let default = MemoryLayout::default();
        assert_eq!(default.max_pages(), default.pages());
        assert!(!default.is_growable());
    }

    /// An unset maximum is the *resolved* size, not the default one: a request
    /// for four pages alone is a fixed four-page memory. Filling it from the
    /// default would cap a four-page memory at one page, which no module can
    /// declare, so the most common partial request would be refused.
    #[test]
    fn an_unset_maximum_is_the_resolved_size() {
        let four = requested(Some(4), None, None).expect("four fixed pages are admissible");
        assert_eq!((four.pages(), four.max_pages()), (4, 4));
        assert!(!four.is_growable());

        let growable = requested(None, Some(8), None).expect("a maximum alone is admissible");
        assert_eq!(
            (growable.pages(), growable.max_pages()),
            (MemoryLayout::default().pages(), 8),
            "a maximum alone raises the ceiling over the default size"
        );
        assert!(growable.is_growable());

        let explicit_fixed =
            requested(Some(2), Some(2), None).expect("a maximum equal to the size is admissible");
        assert!(
            !explicit_fixed.is_growable(),
            "a maximum equal to the size is a fixed memory, however it was spelled"
        );
    }

    /// Each rule about the maximum rejects on its own and names the number it
    /// is about.
    #[test]
    fn the_constructor_rejects_a_broken_maximum() {
        let err = growable(4, 2, 65_536).expect_err("a maximum below the size is refused");
        assert!(err.reason.contains("is below its size"), "{err}");
        assert!(
            err.reason.contains("(2 pages)") && err.reason.contains("(4 pages)"),
            "{err}"
        );

        let err = growable(1, 65_537, 16).expect_err("a maximum past 4 GiB is refused");
        assert!(err.reason.contains("maximum is limited to 65536 pages"), "{err}");
        assert!(err.reason.contains("65537 pages"), "{err}");
    }

    /// The stack must fit in the memory's size, not its maximum: the size is
    /// all a module is guaranteed at instantiation, and growth may fail.
    #[test]
    fn the_stack_must_fit_the_size_rather_than_the_maximum() {
        let err = growable(1, 4, 131_072).expect_err("a 128 KiB stack does not fit one page");
        assert!(
            err.reason.contains("does not fit in the linear memory"),
            "{err}"
        );
    }

    /// The overflow trap's headroom is measured against the maximum, because
    /// growth moves the end of memory up to it. The same stack and size that a
    /// fixed memory holds are refused once the maximum leaves a wrapped frame
    /// pointer nowhere out of bounds to land, and the boundary is exact.
    #[test]
    fn the_headroom_is_measured_against_the_maximum() {
        assert!(
            layout(2, 65_552).is_ok(),
            "a fixed two-page memory holds a stack one frame over a page"
        );
        let err = growable(2, 65_535, 65_552)
            .expect_err("the same stack leaves no headroom under a 65535-page maximum");
        assert!(err.reason.contains("32-bit address space"), "{err}");
        assert!(err.reason.contains("at its maximum (65535 × 64 KiB"), "{err}");
        assert!(err.reason.contains("however far growth moves it"), "{err}");

        let fits = growable(1, 65_535, 65_536).expect("this layout sits exactly on the boundary");
        assert_eq!(
            u64::from(fits.max_pages()) * 65_536 + u64::from(fits.stack_size()),
            1 << 32,
            "this case is meant to sit exactly on the boundary"
        );
        assert!(
            growable(1, 65_535, 65_536 + 16).is_err(),
            "one frame past the boundary must be rejected"
        );
    }

    #[test]
    fn memory_source_selects_the_spelling_the_message_names() {
        assert_eq!(
            MemoryLayoutSource::Manifest.label(),
            "`[memory] pages` / `[memory] max-pages` / `[memory] stack-size`"
        );
        assert_eq!(
            MemoryLayoutSource::Flag.label(),
            "`--memory-pages` / `--max-memory-pages` / `--stack-size`"
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
        let manifest =
            requested(Some(0), None, None).expect_err("a zero-page memory is rejected");
        assert_eq!(
            manifest.to_string(),
            "Invalid `[memory] pages` / `[memory] max-pages` / `[memory] stack-size`: linear \
             memory must be at least one 64 KiB page, but 0 pages were requested"
        );

        let flag = MemoryLayout::resolve(
            MemoryRequest {
                pages: Some(0),
                ..MemoryRequest::default()
            },
            MemoryLayoutSource::Flag,
        )
        .expect_err("a zero-page memory is rejected");
        assert_eq!(
            flag.to_string(),
            "Invalid `--memory-pages` / `--max-memory-pages` / `--stack-size`: linear memory \
             must be at least one 64 KiB page, but 0 pages were requested"
        );
        assert_eq!(
            manifest.reason, flag.reason,
            "the same numbers must be refused for the same reason on either surface"
        );
    }

    /// Asking for an import gives an imported layout of exactly the shape the
    /// same request gives without it, through every numeric accessor;
    /// `is_default_memory`, which deliberately differs, is pinned by the next
    /// test. The import changes where the memory comes from, never what it
    /// holds or where.
    ///
    /// The two layouts still compare unequal: the flag is part of the value, so
    /// a consumer comparing layouts — or hashing one into a cache key — cannot
    /// mistake an imported memory for a defined one.
    #[test]
    fn an_import_request_resolves_to_an_imported_layout() {
        fn numbers(layout: MemoryLayout) -> (u32, u32, u32, bool, u32, u32, i32) {
            (
                layout.pages(),
                layout.max_pages(),
                layout.stack_size(),
                layout.is_stack_requested(),
                layout.data_size(),
                layout.data_base(),
                layout.stack_pointer_init(),
            )
        }

        let cases = [
            ("the default request", MemoryRequest::default()),
            (
                "a size alone",
                MemoryRequest {
                    pages: Some(4),
                    ..MemoryRequest::default()
                },
            ),
            (
                "a maximum alone",
                MemoryRequest {
                    max_pages: Some(8),
                    ..MemoryRequest::default()
                },
            ),
            (
                "a stack alone",
                MemoryRequest {
                    stack_size: Some(32_768),
                    ..MemoryRequest::default()
                },
            ),
            ("all three stated", stated(2, 8, 32_768)),
        ];
        for (case, request) in cases {
            for surface in [MemoryLayoutSource::Manifest, MemoryLayoutSource::Flag] {
                let defined = MemoryLayout::resolve(request, surface)
                    .unwrap_or_else(|err| panic!("{case}: {err}"));
                let imported = MemoryLayout::resolve(imported(request), surface)
                    .unwrap_or_else(|err| panic!("{case}, imported: {err}"));
                assert!(
                    imported.is_imported(),
                    "{case}: asking for the import gives an imported layout"
                );
                assert!(
                    !defined.is_imported(),
                    "{case}: not asking for it gives a defined one"
                );
                assert_eq!(
                    numbers(imported),
                    numbers(defined),
                    "{case}: importing the memory must not change its size, maximum, stack, \
                     data or stack pointer"
                );
                assert_eq!(
                    imported.is_growable(),
                    defined.is_growable(),
                    "{case}: an imported memory is growable exactly when its defined twin is"
                );
                assert_ne!(
                    imported, defined,
                    "{case}: the import is part of the layout's value"
                );
                assert_eq!(
                    defined_twin(imported),
                    defined,
                    "{case}: and it is the only part that differs"
                );
            }
        }
    }

    /// An imported memory is never the default memory, whatever its numbers.
    ///
    /// Code generation omits the default memory from a program that touches
    /// none, so a default verdict here would drop the import too, and the
    /// contract an embedder was written to — supply `env.memory` — would hang
    /// on whether the program happened to touch memory. The defined twin of
    /// every case *is* the default memory, so the flag alone decides; and the
    /// default request and layout are both defined.
    #[test]
    fn an_imported_layout_is_never_the_default_memory() {
        assert!(
            !MemoryRequest::default().import,
            "a request that asks for nothing defines its memory"
        );
        assert!(!MemoryLayout::default().is_imported());
        assert!(
            !MemoryLayout::resolve(MemoryRequest::default(), MemoryLayoutSource::Manifest)
                .expect("the default request resolves")
                .is_imported(),
            "the default request resolves to a defined layout"
        );

        let cases = [
            ("the default request", MemoryRequest::default()),
            (
                "the default stack written out",
                MemoryRequest {
                    pages: Some(1),
                    stack_size: Some(65_536),
                    ..MemoryRequest::default()
                },
            ),
            (
                "the default with its maximum written out",
                stated(1, 1, 65_536),
            ),
        ];
        for (case, request) in cases {
            let defined = MemoryLayout::resolve(request, MemoryLayoutSource::Flag)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            assert!(
                defined.is_default_memory(),
                "{case}: the defined twin is the default memory, so only the flag can decide"
            );
            let imported = MemoryLayout::resolve(imported(request), MemoryLayoutSource::Flag)
                .unwrap_or_else(|err| panic!("{case}, imported: {err}"));
            assert!(imported.is_imported(), "{case}: the import is kept");
            assert!(
                !imported.is_default_memory(),
                "{case}: an imported memory is not the default memory a program may omit"
            );
            assert!(
                !imported
                    .with_static_data(0)
                    .expect("no data always fits")
                    .is_default_memory(),
                "{case}: nor is it once the program's data, here none, has been placed"
            );
        }
    }

    /// An imported layout asks its host for `env.memory`, with the layout's
    /// own size as the minimum and its own maximum as the maximum: the pair a
    /// defined memory would declare. A fixed layout asks for `minimum ==
    /// maximum`, so a host cannot hand over room the layout never measured.
    ///
    /// The name is pinned as literal strings: it is the contract an embedder is
    /// written to, and the one `wasm-ld --import-memory`, Zig and Emscripten
    /// use, so changing either constant breaks every host rather than
    /// refactoring anything.
    #[test]
    fn memory_import_names_env_memory_with_the_layouts_limits() {
        assert_eq!(MemoryImport::MODULE, "env");
        assert_eq!(MemoryImport::FIELD, "memory");

        let cases = [
            (
                "a growable memory",
                MemoryRequest {
                    pages: Some(2),
                    max_pages: Some(8),
                    ..MemoryRequest::default()
                },
                2,
                8,
            ),
            (
                "a fixed memory",
                MemoryRequest {
                    pages: Some(3),
                    ..MemoryRequest::default()
                },
                3,
                3,
            ),
            ("the default numbers", MemoryRequest::default(), 1, 1),
            (
                "a maximum alone",
                MemoryRequest {
                    max_pages: Some(8),
                    ..MemoryRequest::default()
                },
                1,
                8,
            ),
            (
                "a maximum at the headroom limit",
                stated(1, 65_535, 65_536),
                1,
                65_535,
            ),
        ];
        for (case, request, minimum, maximum) in cases {
            let layout = MemoryLayout::resolve(imported(request), MemoryLayoutSource::Flag)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            let import = layout
                .memory_import()
                .unwrap_or_else(|| panic!("{case}: an imported layout asks for an import"));
            assert_eq!(
                import,
                MemoryImport { minimum, maximum },
                "{case}: the import's limits are the layout's size and maximum"
            );
            assert_eq!(
                import.minimum < import.maximum,
                layout.is_growable(),
                "{case}: the import admits growth exactly when the layout is growable"
            );
        }
    }

    /// A defined layout asks its host for nothing, however it was shaped and
    /// whether or not data has been placed in it.
    #[test]
    fn a_defined_layout_has_no_memory_import() {
        let cases = [
            ("the default layout", Ok(MemoryLayout::default())),
            ("a growable layout", growable(2, 4, 65_536)),
            ("a requested stack", requested(None, None, Some(32_768))),
        ];
        for (case, layout) in cases {
            let layout = layout.unwrap_or_else(|err| panic!("{case}: {err}"));
            assert!(
                !layout.is_imported(),
                "{case}: a defined layout is not imported"
            );
            assert_eq!(
                layout.memory_import(),
                None,
                "{case}: a defined layout asks for no import"
            );
            let placed = layout
                .with_static_data(1_024)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            assert_eq!(
                placed.memory_import(),
                None,
                "{case}: placing data does not turn a defined memory into an import"
            );
        }
    }

    /// Placing the program's data keeps the import and the limits the host is
    /// asked for, and places the data exactly as it would in a defined memory:
    /// a requested stack is kept, a default one gives way only when the memory
    /// is full. A refusal is the defined memory's refusal, number for number —
    /// the data is measured against the size, which an import's minimum is.
    #[test]
    fn import_survives_static_data_placement() {
        let cases = [
            (
                "a requested stack beside the data",
                MemoryRequest {
                    max_pages: Some(4),
                    stack_size: Some(32_768),
                    ..MemoryRequest::default()
                },
                32_768,
                32_768,
            ),
            (
                "a default stack giving way to the data",
                MemoryRequest::default(),
                2_000,
                63_536,
            ),
            (
                "a default stack kept in a larger memory",
                MemoryRequest {
                    pages: Some(2),
                    ..MemoryRequest::default()
                },
                2_000,
                65_536,
            ),
            ("no data at all", MemoryRequest::default(), 0, 65_536),
        ];
        for (case, request, data_bytes, stack_size) in cases {
            let resolved = MemoryLayout::resolve(imported(request), MemoryLayoutSource::Manifest)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            let placed = resolved
                .with_static_data(data_bytes)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            assert_eq!(
                placed.stack_size(),
                stack_size,
                "{case}: the stack the data leaves"
            );
            assert!(
                placed.is_imported(),
                "{case}: placing data keeps the import"
            );
            assert_eq!(
                placed.memory_import(),
                resolved.memory_import(),
                "{case}: placing data does not move the limits the host is asked for"
            );
            let defined = MemoryLayout::resolve(request, MemoryLayoutSource::Manifest)
                .unwrap_or_else(|err| panic!("{case}: {err}"))
                .with_static_data(data_bytes)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            assert_eq!(
                defined_twin(placed),
                defined,
                "{case}: the data is placed in an imported memory exactly as in a defined one"
            );
            assert_eq!(
                placed.with_static_data(0),
                Ok(resolved),
                "{case}: re-placing from a layout holding data keeps the import"
            );
        }

        let refusals = [
            (
                "a requested stack that leaves no room",
                MemoryRequest {
                    stack_size: Some(65_536),
                    ..MemoryRequest::default()
                },
                100,
            ),
            (
                "data that leaves a default stack no frame",
                MemoryRequest::default(),
                65_530,
            ),
            (
                "data measured against the size, not the maximum",
                MemoryRequest {
                    max_pages: Some(4),
                    ..MemoryRequest::default()
                },
                65_530,
            ),
        ];
        for (case, request, data_bytes) in refusals {
            let refusal = |request| {
                MemoryLayout::resolve(request, MemoryLayoutSource::Manifest)
                    .unwrap_or_else(|err| panic!("{case}: {err}"))
                    .with_static_data(data_bytes)
                    .expect_err(case)
            };
            assert_eq!(
                refusal(imported(request)),
                refusal(request),
                "{case}: an imported memory refuses the data with the defined memory's numbers"
            );
        }
    }

    /// A fit suggestion is a change list, and no stack or data needs the memory
    /// to come from somewhere else, so the import is never on it: an imported
    /// layout is suggested exactly what its defined twin is, with the import
    /// left unset. A layout no request can fit stays unfittable either way.
    ///
    /// The cases are the ones the defined-memory suggestion is pinned on, plus
    /// a layout that already fits and two that cannot.
    #[test]
    fn a_fit_suggestion_never_names_the_import() {
        let cases = [
            (
                "the memory grows for the data",
                MemoryRequest::default(),
                64_000,
                2_048,
                Some(MemoryRequest {
                    pages: Some(2),
                    ..MemoryRequest::default()
                }),
            ),
            (
                "the stack has to be asked for",
                MemoryRequest::default(),
                80_000,
                0,
                Some(MemoryRequest {
                    pages: Some(2),
                    stack_size: Some(80_000),
                    ..MemoryRequest::default()
                }),
            ),
            (
                "a requested stack large enough stays unstated",
                MemoryRequest {
                    stack_size: Some(32_768),
                    ..MemoryRequest::default()
                },
                16_000,
                40_000,
                Some(MemoryRequest {
                    pages: Some(2),
                    ..MemoryRequest::default()
                }),
            ),
            (
                "a fixed memory stays fixed",
                stated(1, 1, 65_536),
                70_000,
                0,
                Some(MemoryRequest {
                    pages: Some(2),
                    stack_size: Some(70_000),
                    ..MemoryRequest::default()
                }),
            ),
            (
                "a growable memory raises its cap",
                stated(1, 2, 65_536),
                65_536,
                70_000,
                Some(MemoryRequest {
                    pages: Some(3),
                    max_pages: Some(3),
                    ..MemoryRequest::default()
                }),
            ),
            (
                "a layout that already fits changes nothing",
                MemoryRequest::default(),
                1_024,
                0,
                Some(MemoryRequest::default()),
            ),
            (
                "a stack no 32-bit size can hold",
                MemoryRequest::default(),
                u32::MAX,
                0,
                None,
            ),
            (
                "data past the address space",
                MemoryRequest::default(),
                65_536,
                1 << 32,
                None,
            ),
        ];
        for (case, request, stack_bytes, data_bytes, expected) in cases {
            let defined = MemoryLayout::resolve(request, MemoryLayoutSource::Flag)
                .unwrap_or_else(|err| panic!("{case}: {err}"));
            let imported = MemoryLayout::resolve(imported(request), MemoryLayoutSource::Flag)
                .unwrap_or_else(|err| panic!("{case}, imported: {err}"));
            assert!(
                imported.is_imported(),
                "{case}: the layout asked for a suggestion is the imported one"
            );
            let suggestion = imported.request_to_fit(stack_bytes, data_bytes);
            assert!(
                suggestion.is_none_or(|request| !request.import),
                "{case}: a suggestion for an imported layout must not name the import"
            );
            assert_eq!(
                suggestion, expected,
                "{case}: the suggestion is the smallest change, and the import is not part of it"
            );
            assert_eq!(
                suggestion,
                defined.request_to_fit(stack_bytes, data_bytes),
                "{case}: an imported layout is suggested exactly what its defined twin is"
            );
        }
    }

    /// The overflow trap's headroom rule lands on the same side of each exact
    /// boundary for an imported memory, in the same words.
    ///
    /// An imported memory needs no rule of its own because its import declares
    /// the layout's maximum, and WebAssembly import matching admits only a host
    /// memory whose own maximum is no greater: the bound measured here is a
    /// bound on whatever memory the module runs in. So for an accepted layout
    /// the span is measured from the import's maximum, the most memory a host
    /// can hand over, and must still fit in 2^32.
    ///
    /// The growable pair straddles the boundary at two pages. One page cannot
    /// hold a stack one frame past it, so the one-page step is refused by the
    /// stack-fit rule before the headroom rule is reached; it is kept, under
    /// the rule that does fire, because the verdict must match there too.
    #[test]
    fn the_headroom_check_is_the_same_for_an_imported_memory() {
        let headroom = "32-bit address space";
        let cases = [
            (
                "a fixed memory exactly on the boundary",
                stated(65_535, 65_535, 65_536),
                Ok(1 << 32),
            ),
            (
                "a fixed memory one frame past it",
                stated(65_535, 65_535, 65_552),
                Err((headroom, "the linear memory (65535 × 64 KiB")),
            ),
            (
                "a fixed memory filling the address space",
                stated(65_536, 65_536, 65_536),
                Err((headroom, "the linear memory (65536 × 64 KiB")),
            ),
            (
                "a fixed two-page memory with a stack one frame over a page",
                stated(2, 2, 65_552),
                Ok(2 * 65_536 + 65_552),
            ),
            (
                "a one-page memory growable exactly to the boundary",
                stated(1, 65_535, 65_536),
                Ok(1 << 32),
            ),
            (
                "a two-page memory growable exactly to the boundary",
                stated(2, 65_535, 65_536),
                Ok(1 << 32),
            ),
            (
                "a two-page memory growable to one frame past it",
                stated(2, 65_535, 65_552),
                Err((headroom, "at its maximum (65535 × 64 KiB")),
            ),
            (
                "a one-page memory too small for the stack one frame past it",
                stated(1, 65_535, 65_552),
                Err(("does not fit in the linear memory", "65552 bytes")),
            ),
        ];
        for (case, request, expected) in cases {
            for surface in [MemoryLayoutSource::Manifest, MemoryLayoutSource::Flag] {
                match (assert_judged_alike(case, request, surface), expected) {
                    (Ok(layout), Ok(span)) => {
                        let import = layout
                            .memory_import()
                            .unwrap_or_else(|| panic!("{case}: the layout is imported"));
                        let supplied = u64::from(import.maximum) * u64::from(PAGE_SIZE)
                            + u64::from(layout.stack_size());
                        assert_eq!(
                            supplied, span,
                            "{case}: the span is measured from the import's maximum"
                        );
                        assert!(
                            supplied <= 1 << 32,
                            "{case}: the largest memory a host may supply and the stack span \
                             {supplied} bytes, past the 32-bit address space"
                        );
                    }
                    (Err(err), Err((rule, value))) => {
                        assert!(
                            err.reason.contains(rule),
                            "{case}: not the `{rule}` rule: {err}"
                        );
                        assert!(
                            err.reason.contains(value),
                            "{case}: does not name `{value}`: {err}"
                        );
                    }
                    (verdict, _) => panic!(
                        "{case}: the imported memory was judged {verdict:?}, but the expected \
                         verdict is {expected:?}"
                    ),
                }
            }
        }
    }

    /// Every request the constructor is pinned to refuse, and every extreme it
    /// is pinned to accept, gets the same verdict with the memory imported, on
    /// both surfaces — and a refusal reads the same, character for character.
    /// The address-space headroom boundaries are the one exception, pinned with
    /// the import by the headroom test above rather than in this table.
    ///
    /// This is what "the import adds no invariant" means as a check: a rule that
    /// rejected an imported memory, or one that let through an imported memory
    /// a defined one may not have, would split the two verdicts somewhere in
    /// this table. The surface label is unchanged too, pinned by rendering one
    /// imported refusal in full on each surface: the import is not one of the
    /// numbers a refusal can be about.
    #[test]
    fn an_imported_request_is_judged_exactly_as_its_defined_twin() {
        let cases = [
            ("a zero-page memory", stated(0, 0, 65_536), false),
            (
                "more pages than 32-bit WebAssembly allows",
                stated(65_537, 65_537, 65_536),
                false,
            ),
            ("a zero-byte stack", stated(1, 1, 0), false),
            ("a stack off the frame grid", stated(1, 1, 1_000), false),
            (
                "a stack larger than its memory",
                stated(1, 1, 131_072),
                false,
            ),
            (
                "a stack past the signed 32-bit initializer",
                stated(65_536, 65_536, 2_147_483_664),
                false,
            ),
            (
                "a memory filling the address space",
                stated(65_536, 65_536, 65_536),
                false,
            ),
            ("a maximum below the size", stated(4, 2, 65_536), false),
            (
                "a maximum past 32-bit WebAssembly",
                stated(1, 65_537, 16),
                false,
            ),
            (
                "a stack that would fit only the maximum",
                stated(1, 4, 131_072),
                false,
            ),
            (
                "a partial request completing to a stack larger than its memory",
                MemoryRequest {
                    stack_size: Some(131_072),
                    ..MemoryRequest::default()
                },
                false,
            ),
            (
                "a partial zero-page request",
                MemoryRequest {
                    pages: Some(0),
                    ..MemoryRequest::default()
                },
                false,
            ),
            (
                "the largest memory the overflow trap survives",
                stated(65_535, 65_535, 16),
                true,
            ),
            ("a stack smaller than its memory", stated(2, 2, 16), true),
            (
                "the same large stack in a large enough memory",
                MemoryRequest {
                    pages: Some(4),
                    stack_size: Some(131_072),
                    ..MemoryRequest::default()
                },
                true,
            ),
            (
                "a size alone",
                MemoryRequest {
                    pages: Some(4),
                    ..MemoryRequest::default()
                },
                true,
            ),
            (
                "a maximum alone",
                MemoryRequest {
                    max_pages: Some(8),
                    ..MemoryRequest::default()
                },
                true,
            ),
            (
                "a maximum equal to the size",
                MemoryRequest {
                    pages: Some(2),
                    max_pages: Some(2),
                    ..MemoryRequest::default()
                },
                true,
            ),
            ("the default request", MemoryRequest::default(), true),
        ];
        for (case, request, accepted) in cases {
            for surface in [MemoryLayoutSource::Manifest, MemoryLayoutSource::Flag] {
                assert_eq!(
                    assert_judged_alike(case, request, surface).is_ok(),
                    accepted,
                    "{case} on {surface:?}: the table records the defined memory's verdict"
                );
            }
        }

        let zero_pages = imported(MemoryRequest {
            pages: Some(0),
            ..MemoryRequest::default()
        });
        assert_eq!(
            MemoryLayout::resolve(zero_pages, MemoryLayoutSource::Manifest)
                .expect_err("a zero-page memory is refused")
                .to_string(),
            "Invalid `[memory] pages` / `[memory] max-pages` / `[memory] stack-size`: linear \
             memory must be at least one 64 KiB page, but 0 pages were requested"
        );
        assert_eq!(
            MemoryLayout::resolve(zero_pages, MemoryLayoutSource::Flag)
                .expect_err("a zero-page memory is refused")
                .to_string(),
            "Invalid `--memory-pages` / `--max-memory-pages` / `--stack-size`: linear memory \
             must be at least one 64 KiB page, but 0 pages were requested"
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

    /// The two instruction-set predicates a front end holding only a name can
    /// answer, pinned per variant. Both are copies of the emission-side
    /// target's, and the mirror test in `inference-wasm-codegen` is what holds
    /// the copies to the originals; this pins what the copies say, so a change
    /// here is deliberate rather than a silent widening of what a manifest may
    /// ask for.
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
        assert!(!TargetName::Stellar.supports_proof_mode());
        assert!(!TargetName::Stellar.permits_bulk_memory());
        assert!(!TargetName::SpaceWasm.supports_proof_mode());
        assert!(!TargetName::SpaceWasm.permits_bulk_memory());
    }

    /// The two memory predicates, pinned per name for the reason the ones above
    /// are: each is a copy of the emission-side target's, and what a copy says
    /// is what a front end holding only the name acts on.
    ///
    /// Driven from the vocabulary through an exhaustive match, so a name added
    /// to it does not compile here until both answers are written down. The
    /// two are pinned separately because they differ: `stellar` grows a memory
    /// it may not import, so neither predicate can stand in for the other.
    #[test]
    fn each_target_records_whether_it_can_import_or_grow_its_memory() {
        for target in TargetName::ALL {
            let (imports, grows) = match target {
                TargetName::Wasm32 => (true, true),
                TargetName::Stellar => (false, true),
                TargetName::SpaceWasm => (false, false),
            };
            assert_eq!(
                target.supports_imported_memory(),
                imports,
                "`{}` changed whether a build may import its linear memory",
                target.as_str()
            );
            assert_eq!(
                target.permits_memory_growth(),
                grows,
                "`{}` changed whether its runtime can grow a linear memory",
                target.as_str()
            );
        }
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
