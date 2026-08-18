//! Static-merge WASM linker.
//!
//! Inference compiles a program that `use`s functions from an external `.wasm`
//! module into an intermediate module whose extern calls lower to `(import …)`
//! entries (see `core/wasm-codegen` Phase 2). This crate consumes that
//! intermediate module plus the resolved external `.wasm` binaries and produces
//! **one self-contained module** with those imports *satisfied and removed* —
//! the external function bodies are merged in and re-indexed so the output has
//! no dangling cross-module imports.
//!
//! ## What the merge does
//!
//! For each import the main module declares, the linker:
//!
//! 1. finds the external module that exports a function of that name under the
//!    matching logical module,
//! 2. computes the **transitive closure** of that export inside its module (the
//!    functions it calls, the types they reference),
//! 3. classifies the closure into a **feasibility tier** (see [`tier`]),
//! 4. **dedups** the closure's function types into the output type section,
//! 5. **appends** the closure's bodies after the main module's, rewriting every
//!    internal index reference (`call`, `call_indirect` type, …) into the
//!    unified index space,
//! 6. **removes** the satisfied import and redirects the main module's calls to
//!    it onto the merged body.
//!
//! ## Feasibility tiers
//!
//! - **Tier A** — the closure touches no linear memory and names no table.
//!   Merged.
//! - **Tier B** — memory through caller-passed pointers only. Merged onto the
//!   single shared linear memory; no address relocation needed.
//! - **Tier C** — the module declares a data or element segment, or the closure
//!   names the table space. Merging would require relocation metadata the static
//!   merge does not consume, so it is **rejected** with
//!   [`LinkError::RequiresRelocatableBuild`] rather than producing an unsound
//!   module. A table the module merely *declares* and no body touches is inert
//!   and is not a reason; see [`tier`].
//!
//! Globals are classified on use, not declaration: a closure that reads or
//! writes one is Tier A — or Tier B if it also touches memory — and the
//! external's globals are merged into the output above main's with its accessors
//! remapped, an admission kept sound by address provenance tagging a
//! global-derived value `NotParam`, so a closure that computes a memory address
//! through a global is still rejected.
//!
//! That is worth reading before touching either half of it, because two
//! safeguards in this crate exist *for* the global-touching closure and read as
//! redundant to anyone who believes it was excluded here. The provenance rule
//! above is one. The other is that placing an external's data segments at their
//! original addresses stays mutually exclusive with merging globals: an
//! external's shadow-stack region is claimed by nothing this crate parses except
//! the initializer of a mutable global, so a disjointness proof over declared
//! segments and memory limits cannot see it, and a merged global carries that
//! invisible claim into the output. Both arguments are written out in full in the
//! `tier` module documentation. Relaxing either is a soundness change, not a
//! cleanup.
//!
//! ## Entry point
//!
//! [`link`] takes the main module bytes and the external module bytes and
//! returns the unified module bytes. [`link_with_warnings`] is the same merge
//! with the [`LinkWarning`]s it raised, for callers that report them.

mod closure;
mod merge;
mod parse;
mod provenance;
mod rewrite;
mod safety;
mod spec_funcs;
mod tier;

use std::fmt;

use inf_wasmparser::WasmFeatures;
use thiserror::Error;

/// The WebAssembly feature subset the static-merge linker supports.
///
/// The merge copies external function bodies verbatim onto a single shared
/// linear memory, re-indexing only the handful of index-bearing operators, and
/// the paired Rocq translator (`wasm-to-v`) models exactly this machine. That is
/// sound only for the integer **WebAssembly 1.0** core (the MVP plus
/// `mutable-global`) and the two scalar post-MVP additions the merge models:
///
/// - **bulk memory** (`memory.copy` / `memory.fill` over the single memory);
/// - **sign-extension** (`i32.extend8_s`, `i32.extend16_s`, `i64.extend8_s`,
///   `i64.extend16_s`, `i64.extend32_s`).
///
/// Every other *proposal* — reference types, multi-value, tail calls, SIMD,
/// threads/atomics, exception handling, `memory64`, multi-memory, the GC
/// proposal, stack switching, and saturating float-to-int — is
/// **off**. An external using any of them is rejected up front at the link gate
/// with a feature-named [`LinkError::UnsupportedWasmFeature`], rather than late
/// and indirectly when a specific unmodeled opcode happens to reach the merge.
///
/// ## No floating point, anywhere
///
/// The Inference language has no `f32`/`f64` types: its codegen never emits a
/// float operator, a float value type, or a float constant, and the Rocq
/// translator models none of them. Floats are therefore deliberately excluded
/// at the gate. In this `inf-wasmparser` fork, `WasmFeatures::WASM1` bundles
/// `FLOATS` (the baseline float value-type/operator flag) into the MVP set, so
/// this gate cannot name `WASM1` directly: it lists the baseline value-type
/// flags it *does* need and leaves `FLOATS` out. With `FLOATS` off the validator
/// rejects, at the feature pass, any float instruction ("floating-point
/// instruction disallowed") and any float value type in a signature, local, or
/// global ("floating-point support is disabled"). The gate thus encodes a single
/// rule — no floats anywhere, neither operators nor types — enforced before a
/// body is ever copied.
///
/// `GC_TYPES` and `MUTABLE_GLOBAL` are the fork's internal *baseline value-type*
/// flags (`GC_TYPES` gates the GC reference types `externref`/`anyref` — `funcref`
/// is *not* gated by it; `MUTABLE_GLOBAL` admits mutable globals), not WebAssembly
/// proposals, and the validator needs them on to accept ordinary MVP modules.
/// They are therefore deliberately retained. Crucially, `GC_TYPES` being on does
/// **not** admit the GC *proposal*: a GC reference type (`externref`/`anyref`)
/// additionally requires `REFERENCE_TYPES` *and* `GC` (`1 << 19`), neither of
/// which is in this set, and no GC/reference *instruction* survives the allow-list
/// in [`safety`] — every one rejects as an [`LinkError::UnsupportedConstruct`] if
/// it reaches the merge.
/// `STACK_SWITCHING` is likewise off (and defaults off in the fork).
///
/// `SIGN_EXTENSION` is on because the Rocq translator lowers all five of its
/// opcodes (as `BI_unop t (Unop_extend n)` — the proof model treats them as
/// unops, not conversions). Inference codegen still emits none of them, but a
/// real toolchain emits them constantly, and without the flag the validator
/// refuses such an external at this gate *before* the allow-list in [`safety`]
/// ever sees the body. The three integer-to-integer width conversions
/// (`i32.wrap_i64`, `i64.extend_i32_s/u`) need no flag: they are MVP
/// instructions, gated only by the allow-list.
///
/// `SATURATING_FLOAT_TO_INT` stays off. Its operands are floats, the translator
/// declares no float number type, and admitting it here would recreate the
/// allow-listed-but-unlowerable divergence — an external accepted at the gate
/// and at the merge, then failing on the `-v` proof path — that this gate exists
/// to close. An external using it is rejected here with the validator's
/// feature-named diagnostic.
///
/// This is the linker's explicit, enforced supported-version contract: a feature
/// added to the parser later cannot quietly become linkable.
pub const SUPPORTED_WASM_FEATURES: WasmFeatures = WasmFeatures::GC_TYPES
    .union(WasmFeatures::MUTABLE_GLOBAL)
    .union(WasmFeatures::BULK_MEMORY)
    .union(WasmFeatures::SIGN_EXTENSION);

/// Why a static merge could not be produced.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// A module's bytes could not be parsed as WASM.
    #[error("failed to parse WASM module: {0}")]
    Parse(String),

    /// An external module is well-formed WebAssembly but uses a feature outside
    /// the supported [`SUPPORTED_WASM_FEATURES`] subset (e.g. any floating-point
    /// type or instruction, reference types, SIMD, atomics, exceptions,
    /// `memory64`, multi-memory, multi-value, tail calls, or
    /// saturating float-to-int). The merge cannot soundly fold such a module onto
    /// the single shared memory the output models — and the Rocq translator does
    /// not model these constructs — so it is rejected at the link gate with the
    /// validator's feature-named diagnostic rather than later, per unmodeled
    /// opcode.
    #[error(
        "external module `{module}` uses a WebAssembly feature beyond the supported WASM 1.0 subset: {details}"
    )]
    UnsupportedWasmFeature { module: String, details: String },

    /// A required export was not found in any supplied external module.
    #[error("no external module exports a function named `{field}`")]
    UnsatisfiedImport { field: String },

    /// A function in a merged closure calls one of its own module's imports,
    /// which a static merge has no body to satisfy.
    #[error("merged function transitively imports `{module}::{field}`, which has no body to merge")]
    TransitiveHostImport { module: String, field: String },

    /// The external function requires relocation support (Tier C): its module
    /// declares a data or element segment, or its closure names the table space,
    /// so merging it into the shared memory would need relocation metadata. A
    /// table the module merely *declares* and no body touches is not a reason.
    ///
    /// Globals are classified on use, not declaration: a closure that reads or
    /// writes one is Tier A — or Tier B if it also touches memory — and the
    /// external's globals are merged into the output above main's with its
    /// accessors remapped, an admission kept sound by address provenance tagging
    /// a global-derived value `NotParam`, so a closure that computes a memory
    /// address through a global is still rejected. Such a rejection arrives
    /// through the provenance clause of [`crate::tier`] and never names a global
    /// in `reasons`.
    #[error(
        "external function `{field}` requires a relocatable build: {}",
        .reasons.join("; ")
    )]
    RequiresRelocatableBuild { field: String, reasons: Vec<String> },

    /// A WASM construct the static merge does not model (e.g. a reference-typed
    /// value, a non-constant global initializer, or a transitively-imported
    /// environment).
    #[error("unsupported WASM construct for static merge: {0}")]
    UnsupportedConstruct(String),

    /// More than one supplied external module exports a function of the same
    /// field name an import requests, so the body to merge is ambiguous.
    #[error(
        "import `{module}::{field}` is ambiguous: more than one external module exports `{field}`"
    )]
    AmbiguousImport { module: String, field: String },

    /// The merged module failed structural validation. This is a guard against
    /// every effect-scanner gap that would otherwise persist a silently-invalid
    /// artifact: rather than write WASM no runtime accepts, the merge fails with
    /// the validator's diagnostic.
    #[error("merged module failed WASM validation: {0}")]
    InvalidMergedModule(String),

    /// The linear memories of the main module and a merged external could not be
    /// reconciled into one shared output memory. The merge folds every body onto
    /// a single memory; if the modules' memory requirements (minimum pages,
    /// maximum pages, or growth) cannot be satisfied by one memory, the merge
    /// fails rather than emit a module that traps at runtime.
    #[error("cannot reconcile linear memory for `{field}`: {reason}")]
    IncompatibleMemory { field: String, reason: String },
}

/// Something a *successful* link owes the user: the merge produced a valid
/// module, and a guarantee a reader would reasonably expect it to carry does not
/// extend as far as the output does.
///
/// A warning is never a defect the linker found in the merged program. It states
/// where the merge's own proofs stop, at a point where the emitted artifact
/// makes that limit reachable.
///
/// Every variant so far concerns a **merged external**, and a wrapper in the
/// `inference` crate leans on that: its no-externals fast path returns an empty
/// warning list without calling the linker at all, which stays equivalent only
/// while no variant can arise from the main module or the reconciled memory
/// alone. A variant that can must be raised on that path too, or it is silently
/// dropped for every program that links no external.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkWarning {
    /// One or more externals were admitted at Tier B — their memory accesses
    /// are proven to *derive from* the caller's pointers — into an output
    /// reserving more than one 64 KiB page.
    ///
    /// Tier B carries no sizes, so it cannot show an access *stays within* the
    /// buffer the caller granted (see [`provenance`]). With a single page, an
    /// address past that buffer is usually past the memory too and traps; that
    /// backstop is incidental, and a larger memory removes it.
    ///
    /// `fields` names the satisfied import fields, which is how the user knows
    /// these functions.
    TierBInMultiPageMemory { fields: Vec<String>, pages: u64 },
}

impl fmt::Display for LinkWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkWarning::TierBInMultiPageMemory { fields, pages } => {
                let names = fields
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let (subject, verb) = if fields.len() == 1 {
                    ("external", "addresses")
                } else {
                    ("externals", "address")
                };
                write!(
                    f,
                    "merged {subject} {names} {verb} linear memory through caller-supplied \
                     pointers: the merge proves every address derives from a parameter of the \
                     call, not that it stays within the buffer that parameter points into — the \
                     analysis carries no sizes. The reconciled memory reserves {pages} pages, so \
                     an address past the caller's buffer lands in valid memory, possibly a region \
                     another merged module owns, where a single page would usually have trapped. \
                     This states the limit of what the merge proves, not a fault found in the \
                     merged code; issue #420 tracks the containment analysis that would close it."
                )
            }
        }
    }
}

/// A completed merge: the unified module bytes and every [`LinkWarning`] the
/// merge raised.
///
/// Named fields rather than a tuple because the two halves are not
/// interchangeable at a call site — `out.warnings` says what it is, `out.1` does
/// not — and because the warning list is the half a caller may legitimately
/// ignore, which positional access would hide.
#[derive(Debug, Clone)]
pub struct LinkOutput {
    /// The unified, self-contained module.
    pub wasm: Vec<u8>,
    /// Warnings raised during the merge. Empty is the common case.
    pub warnings: Vec<LinkWarning>,
}

/// Merges the satisfiable imports of `main_wasm` from `externals`, returning the
/// unified module together with every warning the merge raised.
///
/// Identical to [`link`] in every respect but the return type; see [`link`] for
/// the resolution rules, the fail-closed contract, and the error conditions.
/// Use this form wherever the warnings can reach the user.
///
/// # Errors
///
/// The same conditions as [`link`].
pub fn link_with_warnings(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
) -> Result<LinkOutput, LinkError> {
    merge::link(main_wasm, externals)
}

/// Merges the satisfiable imports of `main_wasm` from `externals`, returning a
/// single self-contained module with those imports removed.
///
/// Each external is supplied as `(logical_module, bytes)`: the logical,
/// `::`-joined module name the front end bound it under, paired with its `.wasm`
/// bytes. Codegen records every import's `(module, field)` pair, so the merge
/// resolves each import against the external whose logical module matches —
/// never the first external that merely exports the same field name. Two
/// libraries exporting the same field but bound under different logical modules
/// are thereby disambiguated rather than conflated.
///
/// Every import of `main_wasm` must be satisfiable by some external: the merge
/// is **fail-closed**, so an import no external exports is a hard
/// [`LinkError::UnsatisfiedImport`], never a survivor left intact in the output.
/// (The Inference codegen output resolves all its imports before linking, so the
/// live pipeline never trips this; it guards the public API against an
/// unresolved import.)
///
/// Every external is structurally validated (`inf_wasmparser::validate`) at
/// entry, before any closure or provenance work, so this entry point is
/// self-defending against a malformed or adversarial external even when the
/// caller did not pre-validate it.
///
/// This is the **warning-discarding** form of [`link_with_warnings`], kept for
/// callers with nowhere to report a warning to — the test suite, whose subject
/// is the merged bytes, and embedders that only want the artifact. Anything that
/// speaks to a user should call [`link_with_warnings`] instead: a discarded
/// [`LinkWarning`] is a claim about the artifact the user never hears.
///
/// # Errors
///
/// Returns a [`LinkError`] if any module fails to parse or an external fails
/// structural validation ([`LinkError::Parse`]), a merged closure reaches a
/// host import, a closure falls into the unsupported Tier C, or more than one
/// external is bound under the same `(module, field)` pair an import names
/// ([`LinkError::AmbiguousImport`]).
pub fn link(main_wasm: &[u8], externals: &[(&str, &[u8])]) -> Result<Vec<u8>, LinkError> {
    link_with_warnings(main_wasm, externals).map(|out| out.wasm)
}

/// Validates one external `.wasm` against the linker's supported-version
/// contract, the same two-pass gate [`link`] applies to every external before it
/// is merged.
///
/// The check runs in two passes so the diagnostic is precise:
///
/// 1. **Structural** validation under the parser's default features distinguishes
///    genuinely malformed bytes ([`LinkError::Parse`]) from a well-formed module
///    that merely uses a newer feature.
/// 2. **Feature** validation under [`SUPPORTED_WASM_FEATURES`] rejects a
///    well-formed module that uses any proposal beyond the supported WASM 1.0
///    subset ([`LinkError::UnsupportedWasmFeature`], whose message names the
///    feature).
///
/// Exposed so the CLI driver can reject a non-1.0 external at the earliest point
/// with the *same* feature-named diagnostic the linker uses — keeping the gate a
/// single source of truth rather than two divergent validations.
///
/// # Errors
///
/// Returns [`LinkError::Parse`] for structurally invalid bytes, or
/// [`LinkError::UnsupportedWasmFeature`] for a well-formed module outside the
/// supported subset.
pub fn validate_external(logical_module: &str, bytes: &[u8]) -> Result<(), LinkError> {
    merge::validate_external(logical_module, bytes)
}
