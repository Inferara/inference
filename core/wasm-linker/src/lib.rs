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

    /// A merged external may store through a parameter its `external fn`
    /// declaration did not declare `mut` (see [`ImportWriteSet`]).
    ///
    /// Both `module` and `field` are carried because neither identifies the
    /// external on its own: two libraries bound under different logical modules
    /// may export the same field, and the whole error is printed verbatim by
    /// `infc`.
    #[error("{}", render_undeclared_write(.module, .field, *.param_index, .param_name.as_deref()))]
    UndeclaredExternWrite {
        module: String,
        field: String,
        param_index: u32,
        /// The declared name of the offending parameter, or `None` when the
        /// declaration wrote it in an unnamed form — which has no place to put
        /// `mut`, so naming it is a required first step.
        ///
        /// `None` means *unnamed*, never *undescribed*: an import no contract
        /// entry covers is [`LinkError::UndescribedExternWrite`], a different
        /// situation with a different repair.
        param_name: Option<String>,
    },

    /// A merged external may store through a parameter, and the checked mode's
    /// contract list does not describe this import at all — no entry covers it,
    /// named or unnamed.
    ///
    /// Held apart from [`LinkError::UndeclaredExternWrite`] because the repair
    /// is not the same one. That error speaks to an author whose declaration the
    /// linker read and found wanting; this one arises when no declaration
    /// reached the linker, so telling the author to add `mut` to a parameter —
    /// or to name an unnamed one — would describe a declaration that plays no
    /// part in the verdict. On the live pipeline every bound import contributes
    /// an entry, so this reports a gap between the front end and the linker
    /// rather than a source defect, and it fails closed: an undescribed import
    /// is held to writing nothing, never exempted.
    #[error(
        "merged external `{module}::{field}` may store through the address parameter \
         {param_index} denotes, and the write-set contract supplied for this link describes no \
         such import; an import nothing declares is held to writing nothing, so the store is \
         refused rather than admitted unchecked — declare the import with an `external fn` whose \
         parameter {param_index} is `mut`, and bind it, so the declaration reaches the linker"
    )]
    UndescribedExternWrite {
        module: String,
        field: String,
        param_index: u32,
    },

    /// The caller's contract list holds more than one entry for the same
    /// `(module, field)` pair.
    ///
    /// The list is a map written as a slice, and an import is satisfied on
    /// exactly one `(module, field)`, so two entries for one key leave no basis
    /// to choose between them: a permissive entry and a restrictive one would
    /// otherwise decide the link by whichever came first in the slice, silently.
    ///
    /// Rejecting matches what the front end already guarantees. It folds two
    /// declarations of one import into a single entry when they agree, and
    /// reports a hard error naming both files when they do not, so a list it
    /// produced never carries a duplicate key. A duplicate therefore means the
    /// caller has not settled the disagreement, which is not a question the
    /// linker can answer for it: a union licenses a write the restrictive entry
    /// refuses, an intersection refuses one the permissive entry allows, and
    /// first-match is the order dependence itself.
    #[error(
        "the write-set contract supplied for this link holds more than one entry for import \
         `{module}::{field}`; an import is satisfied on exactly one `(module, field)` pair, so \
         two entries for it leave no basis to choose which governs — supply a single entry per \
         import"
    )]
    DuplicateWriteContract { module: String, field: String },

    /// A proof obligation the main module carries applies a function symbol
    /// that no function of the merged module is named.
    ///
    /// Raised here rather than left to the proof translator because this is the
    /// last phase that knows what the symbol was meant to name: the merge writes
    /// no import section, so downstream there is no record of which imports were
    /// satisfied, from which logical module, or under which export field, and
    /// the only honest report left is the symbol itself.
    #[error("{}", render_unresolved_obligation(.symbol, .merged_roots))]
    UnresolvedObligationSymbol {
        symbol: String,
        /// Every `{logical_module}::{export_field}` root the merge produced,
        /// ascending — what the module does offer, against what was asked for.
        /// Always recorded; only *rendered* for a symbol in the merged half of
        /// the name section, because a symbol naming one of the program's own
        /// functions has no import behind it and listing the satisfied ones
        /// would point at the wrong place. A caller matching on the variant
        /// still sees the full list either way.
        merged_roots: Vec<String>,
    },

    /// A proof obligation applies a function symbol that more than one function
    /// of the merged module is named.
    ///
    /// The translator resolves an applied symbol by name, so two carriers make
    /// the obligation describe whichever one the lookup reaches — and a *true*
    /// obligation about the wrong body is worse than a false one, because it
    /// discharges. `carriers` says where each came from, which is knowledge the
    /// merge holds and the translator does not.
    #[error("{}", render_ambiguous_obligation(.symbol, .carriers))]
    AmbiguousObligationSymbol {
        symbol: String,
        /// One line per carrier, in output-index order.
        carriers: Vec<String>,
    },
}

/// Renders [`LinkError::UnresolvedObligationSymbol`].
///
/// The two subspaces of the merged `name` section fail for different reasons and
/// have different repairs, and the symbol says which one it belongs to: a merged
/// external body's name carries `::`, which no compiled Inference function's
/// name can, because every segment of one is an identifier and the joiner is a
/// dot.
fn render_unresolved_obligation(symbol: &str, merged_roots: &[String]) -> String {
    if !symbol.contains(inference_fn_key::MERGED_SEPARATOR) {
        return format!(
            "a proof obligation applies function symbol `{symbol}`, which no function of the \
             merged module carries. The symbol carries no `::`, so it names one of the program's \
             own functions rather than a merged external body — check it against the name code \
             generation writes into the name section for that function: a non-entry-file \
             function's is file-qualified (`lib.arith.add`), and a specification function's is \
             bare"
        );
    }
    let offered = if merged_roots.is_empty() {
        "this merge satisfied no imports, so it contributed no external bodies at all".to_string()
    } else {
        format!(
            "this merge satisfied {}",
            merged_roots
                .iter()
                .map(|root| format!("`{root}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "a proof obligation applies function symbol `{symbol}`, which names a merged external \
         body, and no body this merge produced carries that name — {offered}. The symbol is the \
         logical module and the export field of a bound `external fn`, so check that the \
         declaration the obligation was written against is bound from that module under that \
         field"
    )
}

/// Renders [`LinkError::AmbiguousObligationSymbol`].
fn render_ambiguous_obligation(symbol: &str, carriers: &[String]) -> String {
    format!(
        "a proof obligation applies function symbol `{symbol}`, which {} functions of the \
         merged module carry: {}. An obligation names exactly one body, and nothing downstream \
         can choose between them — the proof translator sees the symbol, not where each \
         function came from",
        carriers.len(),
        carriers.join("; ")
    )
}

/// Renders [`LinkError::UndeclaredExternWrite`], which has to teach a different
/// repair depending on how the declaration spells the offending parameter.
///
/// A named parameter takes `mut` directly. An unnamed one (`_: T`, or a bare
/// type) carries no mutability field and the grammar has no slot for one, so a
/// write set is unspellable until the parameter is given a name — and that fix
/// has to be said, because no other one exists.
///
/// Both branches speak to an author whose declaration the linker read. An import
/// the contract list never described is not one of them and does not arrive here
/// — it is [`LinkError::UndescribedExternWrite`], whose repair is to get a
/// declaration to the linker at all.
fn render_undeclared_write(
    module: &str,
    field: &str,
    param_index: u32,
    param_name: Option<&str>,
) -> String {
    let subject = match param_name {
        Some(name) => format!("parameter {param_index} (`{name}`)"),
        None => format!("parameter {param_index}"),
    };
    let fix = match param_name {
        Some(name) => format!(
            "declare it `mut {name}` in the `external fn` declaration, and pass a `mut` binding at \
             every call"
        ),
        None => format!(
            "the declaration writes parameter {param_index} in an unnamed form, which has nowhere \
             to put `mut`: give it a name first, then declare it `mut name: type` and pass a `mut` \
             binding at every call"
        ),
    };
    format!(
        "merged external `{module}::{field}` may store through the address {subject} denotes, \
         which its `external fn` declaration does not declare `mut`; a foreign store through a \
         caller's pointer is invisible in Inference source, so the declaration is what states it — \
         {fix}"
    )
}

/// What an `external fn` declaration says a merged body may write through.
///
/// The linker derives, from the merged bytes, which of an exported function's
/// parameters its closure may *store* through; this is the declaration that
/// claim is checked against. `mut` on an `external fn` parameter declares that
/// the foreign body may store through the address that parameter denotes, and
/// nothing else in the artifact records it — a custom section would not survive
/// `wasm-opt`, and the merge re-emits a fixed set of sections — so the contract
/// travels beside the bytes instead.
///
/// Keyed on `(module, field)`, the same pair an import is satisfied on. The two
/// parallel vectors are in **WASM parameter order**, which for an `external fn`
/// is the declaration's own argument order: every argument form occupies one
/// parameter slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWriteSet {
    /// Logical module the import was bound under.
    pub module: String,
    /// Export field the import names.
    pub field: String,
    /// WASM parameter indices declared `mut`.
    pub mut_params: Vec<u32>,
    /// Declared parameter names, positionally. `None` for a parameter written
    /// in an unnamed form, which cannot carry `mut` at all — a rejection quotes
    /// this to teach the name-it-first repair.
    pub param_names: Vec<Option<String>>,
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
/// the resolution rules, the fail-closed contract, the two `contracts` modes,
/// and the error conditions. Use this form wherever the warnings can reach the
/// user.
///
/// # Errors
///
/// The same conditions as [`link`].
pub fn link_with_warnings(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
) -> Result<LinkOutput, LinkError> {
    merge::link(main_wasm, externals, contracts)
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
/// # The two `contracts` modes
///
/// `contracts` decides whether the write-set check ([`ImportWriteSet`]) runs at
/// all, and the choice is an explicit mode rather than an emptiness test:
///
/// * `None` — **merge mechanics only.** No declaration governs these imports, so
///   the check does not run. This is the mode for a caller that has no Inference
///   source behind the main module: a hand-written `.wasm`, the fuzz target, and
///   the tests whose subject is the merge itself.
/// * `Some(list)` — **checked.** Every satisfied import is held to a declared
///   write set. An import `list` does not mention is held to the claim that it
///   writes nothing, so any store through a parameter rejects it; a missing
///   entry is never a skipped check. It rejects under its own
///   [`LinkError::UndescribedExternWrite`], because the repair for an import
///   nothing described is not the one for a declaration that covered too little.
///
/// An `Option` rather than an empty slice, because this is the one entry point:
/// [`link_with_warnings`] and this function are the same merge, so an empty
/// slice would have to mean both "check nothing, nothing was declared" and
/// "check everything, nothing may be written".
///
/// `list` is a map written as a slice, and it is validated as one: two entries
/// for the same `(module, field)` are refused
/// ([`LinkError::DuplicateWriteContract`]) rather than resolved by their order,
/// which would otherwise let the same bytes link or fail depending on which
/// entry the caller wrote first.
///
/// # Errors
///
/// Returns a [`LinkError`] if any module fails to parse or an external fails
/// structural validation ([`LinkError::Parse`]), a merged closure reaches a
/// host import, a closure falls into the unsupported Tier C, more than one
/// external is bound under the same `(module, field)` pair an import names
/// ([`LinkError::AmbiguousImport`]), or — in the checked mode — `contracts`
/// holds two entries for one import ([`LinkError::DuplicateWriteContract`]), a
/// merged closure may store through a parameter its declaration did not declare
/// `mut` ([`LinkError::UndeclaredExternWrite`]), or a merged closure stores and
/// no entry described its import at all
/// ([`LinkError::UndescribedExternWrite`]).
///
/// A main module carrying proof obligations is additionally held to naming
/// functions the merged module has: an applied symbol no function of the output
/// carries is [`LinkError::UnresolvedObligationSymbol`], and one two functions
/// share is [`LinkError::AmbiguousObligationSymbol`]. Both are checked here
/// because the merge is the last phase that knows which import supplied which
/// body.
pub fn link(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
) -> Result<Vec<u8>, LinkError> {
    link_with_warnings(main_wasm, externals, contracts).map(|out| out.wasm)
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
