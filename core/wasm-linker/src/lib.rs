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
//! [`link_with_options`] takes the main module bytes, the external module bytes
//! and the policy inputs, and returns the unified module together with the
//! [`LinkWarning`]s the merge raised. [`link_with_warnings`] is that with the
//! default options, and [`link`] is that with the warnings discarded.
//!
//! ## A linked library's own proof obligations
//!
//! A library compiled in proof mode ships `inference.spec_funcs` and
//! `inference.hspecs` sections describing **its own** code. Only the executable
//! closure of a satisfied export crosses the merge, and a library's
//! specification functions are never in it, so those sections describe a module
//! the output is not. What the merge does about that is an explicit input,
//! [`LinkOptions::external_specs`], with three settings:
//!
//! - [`ExternalSpecPolicy::Warn`] (the default) — neither section is decoded and
//!   a [`LinkWarning::ExternalSpecsDropped`] names each contributing library
//!   whose obligations the output does not carry.
//! - [`ExternalSpecPolicy::Ignore`] — the same bytes, and nothing said.
//! - [`ExternalSpecPolicy::Adopt`] — each contributing library's sections are
//!   decoded and its **universal** (`forall`) obligations are written into the
//!   merged module's own sections, keyed under the logical module the library
//!   was bound from, with every applied symbol rewritten onto the merged body it
//!   names. Reachability obligations are reported and left behind.

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
    /// of the merged module answers to.
    ///
    /// The translator resolves an applied symbol by name, so two carriers make
    /// the obligation describe whichever one the lookup reaches — and a *true*
    /// obligation about the wrong body is worse than a false one, because it
    /// discharges. `carriers` says where each came from, which is knowledge the
    /// merge holds and the translator does not.
    ///
    /// A carrier is usually a `name` section entry, but not always: one foreign
    /// body may satisfy several imports and the section holds one name per
    /// index, so a body can be bound under a field the section could not record.
    /// A symbol naming such a field *and* a function the section does name has
    /// two readings and raises this too, even though only one entry spells it.
    #[error("{}", render_ambiguous_obligation(.symbol, .carriers))]
    AmbiguousObligationSymbol {
        symbol: String,
        /// One line per carrier, in output-index order.
        carriers: Vec<String>,
    },

    /// Adoption only: the library ships obligations under a specification its
    /// own `inference.spec_funcs` section does not list, so its two verification
    /// sections disagree with each other.
    ///
    /// Refused rather than adopted, because the merged module would satisfy the
    /// same cross-invariant only by construction — the adopted key is written
    /// into both sections at once — which is exactly the laundering of a
    /// producer defect a link must not perform.
    #[error(
        "linked module `{module}` ships obligations under specification `{spec}`, which its own \
         `inference.spec_funcs` section does not list. A library's two verification sections must \
         agree — the obligations are a subset of the specifications — so this artifact is \
         inconsistent as its own producer wrote it, and the proof translation rejects such a \
         module outright. Adopting from it would carry a claim its producer did not record, into \
         a merged module that satisfies the invariant only because the adopted key is written \
         into both sections at once. Rebuild the library, or link without adopting its \
         specifications"
    )]
    AdoptedSpecUnlisted { module: String, spec: String },

    /// Adoption only: the name an adopted specification would take is not one
    /// the proof translation can spell as an identifier.
    ///
    /// `reason` is the structural clause, a lowercase unpunctuated sentence
    /// fragment set inside the message.
    #[error(
        "adopting the specifications of linked module `{module}` would name one of them \
         `{key}`, which the proof translation cannot use as an identifier: {reason}. An adopted \
         specification's name is the logical module the library is bound under, underscore-joined \
         onto the name the library gave the specification, so it is fixed by those two — rename \
         the specification in the library, bind the library under a different logical module, or \
         link without adopting its specifications"
    )]
    AdoptedSpecNameInvalid {
        module: String,
        spec: String,
        key: String,
        reason: String,
    },

    /// Adoption only: the name an adopted specification would take is already
    /// claimed — by a specification the program declares (`contender: None`) or
    /// by another library's adopted specification.
    ///
    /// One variant for both because the fault is one fault: two sets of
    /// obligations would become one entry in the merged proof artifact. The
    /// repairs differ, which is why the rendering branches.
    #[error("{}", render_adopted_name_collision(.spec, .module, .contender.as_deref()))]
    AdoptedSpecNameCollision {
        /// The adopted key both would take.
        spec: String,
        /// The library whose specification would take it.
        module: String,
        /// The other library that reaches the same key, or `None` when the
        /// contender is the program's own specification.
        contender: Option<String>,
    },

    /// Adoption only: an adopted obligation applies a symbol no function of its
    /// own library's `name` section carries.
    #[error(
        "the obligation linked module `{module}` ships under specification `{spec}` applies \
         function symbol `{symbol}`, which no function of that module's `name` section carries. \
         An adopted obligation is resolved against the library's own name section before it is \
         pointed at a merged body, so a symbol the library does not name cannot be pointed \
         anywhere — leaving it as written would let it resolve against a function of the program \
         that happens to share the name, which is a true obligation about the wrong body. \
         Rebuild the library with its `name` section intact, or link without adopting its \
         specifications"
    )]
    AdoptedObligationSymbolUnresolved {
        module: String,
        spec: String,
        symbol: String,
    },

    /// Adoption only: several functions of the library carry the symbol an
    /// adopted obligation applies, so nothing can say which body it is about.
    #[error("{}", render_adopted_symbol_ambiguous(.module, .spec, .symbol, .carriers))]
    AdoptedObligationSymbolAmbiguous {
        module: String,
        spec: String,
        symbol: String,
        /// The library's own function indices carrying the symbol, ascending.
        carriers: Vec<u32>,
    },

    /// Adoption only: an adopted obligation applies a function of its library
    /// that this merge did not fold in — one outside the export closure
    /// (`imported: false`) or one of the library's own imports
    /// (`imported: true`).
    ///
    /// One variant with a branching render rather than two, because the fault is
    /// the same one (the merged module holds no body for the obligation to be
    /// about) and only the repair differs.
    #[error("{}", render_adopted_unmerged_symbol(.module, .spec, .symbol, *.imported))]
    AdoptedObligationUnmergedSymbol {
        module: String,
        spec: String,
        symbol: String,
        /// Whether the symbol names one of the library's own imports rather than
        /// a function it defines.
        imported: bool,
    },

    /// Adoption only: the merged-namespace symbol an adopted specification
    /// function would take is one a function of the merged output already
    /// carries.
    #[error(
        "adopting the obligation linked module `{module}` ships under specification `{spec}` \
         would record its specification function as `{symbol}`, a name a function of the merged \
         module already carries. A universal obligation's own symbol is not resolved by the proof \
         translation today, so the collision changes nothing yet — which is exactly why it is \
         refused here rather than left for the day something does resolve it, when the obligation \
         would silently describe that function instead. Rebuild the library with a different \
         specification function name, or link without adopting its specifications"
    )]
    AdoptedSpecSymbolCollision {
        module: String,
        spec: String,
        symbol: String,
    },
}

/// Renders [`LinkError::AdoptedSpecNameCollision`], whose contenders lead to
/// different repairs: the program's own specification can be renamed by the
/// person reading this, and another library's cannot.
///
/// Two externals may legitimately be bound under one logical module — only an
/// ambiguous `(module, field)` pair is refused — so the contender can be the
/// module itself. Naming it twice and prescribing a different logical module
/// would print one name as though it were two and ask for a change that is
/// already true, so that case gets a rendering of its own.
fn render_adopted_name_collision(spec: &str, module: &str, contender: Option<&str>) -> String {
    match contender {
        None => format!(
            "adopting the specifications of linked module `{module}` would name one of them \
             `{spec}`, which is already the name of a specification this program declares. The \
             two would become one entry in the merged proof artifact and one set of obligations \
             would be lost at exit 0, so the adoption is refused rather than resolved — rename \
             the program's own specification, or bind the library under a different logical module"
        ),
        Some(contender) if contender == module => format!(
            "two libraries bound under the logical module `{module}` each ship a specification \
             that would be adopted as `{spec}`. An adopted specification's name folds the \
             logical module onto the specification's own name with `_`, so two libraries under \
             one logical module reach one name whenever they name a specification alike — and \
             gathering their obligations into a single `ValidSpec` list would leave nothing \
             downstream able to tell which came from where. Bind one of the two libraries under \
             a logical module of its own"
        ),
        Some(contender) => format!(
            "adopting the specifications of linked modules `{module}` and `{contender}` would \
             name one of each `{spec}`. An adopted specification's name folds the logical module \
             onto the specification's own name with `_`, a join that is not injective, so two \
             libraries can reach one name — and gathering their obligations into a single \
             `ValidSpec` list would leave nothing downstream able to tell which came from where. \
             Bind one of the two libraries under a different logical module"
        ),
    }
}

/// Renders [`LinkError::AdoptedObligationSymbolAmbiguous`].
fn render_adopted_symbol_ambiguous(
    module: &str,
    spec: &str,
    symbol: &str,
    carriers: &[u32],
) -> String {
    let indices = carriers
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "the obligation linked module `{module}` ships under specification `{spec}` applies \
         function symbol `{symbol}`, which {} functions of that module carry `name`-section \
         entries for (indices {indices}). An obligation names exactly one body, and nothing here \
         can choose between them — the rewrite onto the merged output would pick one silently, \
         and a *true* obligation about the wrong body is worse than a false one, because it \
         discharges. Rebuild the library with distinct function names, or link without adopting \
         its specifications",
        carriers.len()
    )
}

/// Renders [`LinkError::AdoptedObligationUnmergedSymbol`]. A function the
/// closure did not reach can be brought in by importing something that reaches
/// it; a library's own import can never be brought in at all, so the two say
/// different things.
fn render_adopted_unmerged_symbol(
    module: &str,
    spec: &str,
    symbol: &str,
    imported: bool,
) -> String {
    if imported {
        return format!(
            "the obligation linked module `{module}` ships under specification `{spec}` applies \
             function symbol `{symbol}`, which is an import of that module rather than one of its \
             own functions. A static merge has no body to splice in for a library's own import, \
             so the merged module holds nothing the obligation could be about. Link without \
             adopting its specifications"
        );
    }
    format!(
        "the obligation linked module `{module}` ships under specification `{spec}` applies its \
         own function `{symbol}`, which this merge did not fold in: only the bodies a satisfied \
         import transitively reaches cross the merge, and nothing this program imports from \
         `{module}` reaches `{symbol}`. The merged module holds no body for the obligation to be \
         about, so adopting it would state a claim over a function the artifact does not contain. \
         Import a function of `{module}` whose closure reaches `{symbol}`, or link without \
         adopting its specifications"
    )
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

    /// One or more linked libraries ship proof obligations of their own that
    /// this merge did not carry into the output.
    ///
    /// Keyed on `inference.hspecs` alone. A library carrying only
    /// `inference.spec_funcs` records spec *membership* — indices of its own
    /// specification functions, which are outside every export closure and so
    /// name nothing the output contains — and loses nothing worth reporting.
    ///
    /// `modules` names only libraries that supplied at least one merged body:
    /// one nothing imports from contributes nothing to the artifact, so nothing
    /// about it was dropped in a sense the reader can act on. That scope is what
    /// keeps this variant inside the promise this type's own documentation makes
    /// and `core/inference`'s no-externals fast path rests on.
    ///
    /// The message states presence and never a count: under this policy the
    /// section is not decoded, and reporting a number would mean decoding on the
    /// path that exists precisely so it does not have to. Precision follows
    /// decoding.
    ExternalSpecsDropped { modules: Vec<String> },

    /// Adoption carried a library's universal obligations and left its
    /// reachability (`exists` / `unique`) obligations behind.
    ///
    /// One instance per contributing library. Each `obligations` entry names the
    /// library's own specification and the obligation's own function symbol,
    /// with its kind.
    ///
    /// A warning rather than a hard error because the program author cannot
    /// repair a reachability obligation in someone else's library: the only
    /// user-side response to a rejection would be turning adoption off
    /// entirely, throwing away the universal half that adopted fine.
    ReachabilityObligationsNotAdopted {
        module: String,
        /// How many of that library's specifications this link did adopt. Zero
        /// when every obligation it ships is a reachability obligation, which
        /// is the case the closing clause must not describe as a partial
        /// success.
        adopted: usize,
        obligations: Vec<String>,
    },
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
            LinkWarning::ExternalSpecsDropped { modules } => {
                let names = modules
                    .iter()
                    .map(|module| format!("`{module}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let opening = if modules.len() == 1 {
                    format!(
                        "linked module {names} ships proof obligations of its own (an \
                         `inference.hspecs` section)"
                    )
                } else {
                    format!(
                        "linked modules {names} ship proof obligations of their own (an \
                         `inference.hspecs` section each)"
                    )
                };
                write!(
                    f,
                    "{opening}, and this link carried none of them into the merged module: the \
                     merge folds in only the executable bodies a satisfied import reaches, so the \
                     library's own assertions are absent from the merged module's own \
                     verification sections. Nothing about the merged code changed — the \
                     obligations were never part of it — and this is not a fault found in the \
                     library. To carry the library's universal (`forall`) obligations into this \
                     program's proof artifact, pass `--adopt-external-specs` to a proof-mode \
                     `infc` build (`-v`, or `--mode proof`), or set `[verification] \
                     adopt-external-specs = true` in Inference.toml. That build reports every \
                     obligation it could not carry, including the case where the library ships \
                     no universal obligation at all and adoption therefore carries nothing."
                )
            }
            LinkWarning::ReachabilityObligationsNotAdopted {
                module,
                adopted,
                obligations,
            } => {
                let count = obligations.len();
                let left_behind = if count == 1 {
                    "1 reachability obligation was".to_string()
                } else {
                    format!("{count} reachability obligations were")
                };
                let opening = if *adopted == 0 {
                    format!(
                        "adopting the specifications of linked module `{module}` carried nothing: \
                         {left_behind} left behind, and it ships no universal (`forall`) \
                         obligation to carry in their place"
                    )
                } else {
                    format!(
                        "adopting the specifications of linked module `{module}` carried its \
                         universal (`forall`) obligations only; {left_behind} left behind"
                    )
                };
                let closing = if *adopted == 0 {
                    "Nothing from this library reached the proof artifact, and nothing was found \
                     wrong with the library."
                } else {
                    "The universal obligations were adopted and are unaffected; nothing was \
                     found wrong with the library."
                };
                write!(
                    f,
                    "{opening}: {}. An `exists` or `unique` judgment is evaluated against the \
                     frame an execution of its specification function reaches, and a \
                     specification function never crosses the merge — only the executable \
                     closure of a satisfied import does — so an adopted reachability obligation \
                     would name a function this module does not contain, and the proof \
                     translation would reject the artifact outright. {closing}",
                    obligations.join(", ")
                )
            }
        }
    }
}

/// What a link does with the verification sections a linked external carries.
///
/// A library compiled in proof mode ships `inference.spec_funcs` and
/// `inference.hspecs` describing *its own* code. Only the executable closure of
/// a satisfied export crosses the merge, so those sections describe a module the
/// output is not, and the merge has never carried them. This says what it does
/// about that instead — and it is an explicit input rather than something the
/// linker infers from the bytes, because "is this build going to state proof
/// obligations" is a fact about the caller's intent that no module carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExternalSpecPolicy {
    /// Drop them without a word. For a build that writes no proof artifact,
    /// where a report about obligations nothing would have consumed is noise on
    /// every compile of every program that links a proof-mode library.
    Ignore,
    /// Drop them and say so, once per link. The default.
    ///
    /// The bytes a merge emits are **identical** under [`Self::Ignore`] and this
    /// — the policy decides only what is *said*. So defaulting here cannot
    /// change any caller's artifact, while defaulting to [`Self::Ignore`] would
    /// preserve today's silence forever for every caller that never learns the
    /// option exists. The default is the one that fails loud, at zero cost to
    /// output stability. A caller that has decided the drop is uninteresting
    /// says so with [`Self::Ignore`]; a caller that has decided nothing must not
    /// be the one that suppresses the notice.
    #[default]
    Warn,
    /// Carry each contributing library's **universal** (`forall`) obligations
    /// into the merged module's own `inference.spec_funcs` /
    /// `inference.hspecs`, keyed under the logical module it was bound from,
    /// with every applied symbol rewritten onto the merged body it names.
    /// Reachability (`exists`/`unique`) obligations are not adoptable and are
    /// reported dropped.
    Adopt,
}

/// The policy inputs a merge takes beyond its modules and its write-set
/// contract.
///
/// Deliberately **not** `#[non_exhaustive]`: a field added here governs what
/// enters a proof artifact, and a struct literal that stops compiling is the
/// loud notice that decision deserves. `#[non_exhaustive]` would instead let
/// every existing caller keep compiling with the new field silently defaulted —
/// the same argument the exhaustive matches in the merge pass are written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkOptions {
    /// What the merge does with the verification sections a linked external
    /// carries.
    pub external_specs: ExternalSpecPolicy,
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

/// Merges the satisfiable imports of `main_wasm` from `externals` under the
/// given policy inputs, returning the unified module together with every warning
/// the merge raised.
///
/// The one entry point: [`link_with_warnings`] is this with
/// [`LinkOptions::default`], and [`link`] is that with the warnings discarded.
/// See [`link`] for the resolution rules, the fail-closed contract, the two
/// `contracts` modes, and the error conditions.
///
/// `options` is taken by reference although the struct is `Copy` today, so a
/// later field that is not `Copy` does not churn four signatures.
///
/// # Errors
///
/// The same conditions as [`link`], plus — under
/// [`ExternalSpecPolicy::Adopt`] alone — every way an adoption can be refused:
/// a library whose two verification sections disagree
/// ([`LinkError::AdoptedSpecUnlisted`]), a key the proof translation cannot
/// spell ([`LinkError::AdoptedSpecNameInvalid`]) or that is already claimed
/// ([`LinkError::AdoptedSpecNameCollision`]), an applied symbol the library's
/// own `name` section carries on no function
/// ([`LinkError::AdoptedObligationSymbolUnresolved`]) or on several
/// ([`LinkError::AdoptedObligationSymbolAmbiguous`]), an obligation over a body
/// this merge did not fold in ([`LinkError::AdoptedObligationUnmergedSymbol`]),
/// an adopted specification symbol the merged output already carries
/// ([`LinkError::AdoptedSpecSymbolCollision`]), and a malformed or duplicated
/// external verification section ([`LinkError::Parse`], naming the logical
/// module).
pub fn link_with_options(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
    options: &LinkOptions,
) -> Result<LinkOutput, LinkError> {
    merge::link(main_wasm, externals, contracts, options)
}

/// Merges the satisfiable imports of `main_wasm` from `externals`, returning the
/// unified module together with every warning the merge raised.
///
/// Identical to [`link`] in every respect but the return type; see [`link`] for
/// the resolution rules, the fail-closed contract, the two `contracts` modes,
/// and the error conditions. Use this form wherever the warnings can reach the
/// user.
///
/// The defaulting form of [`link_with_options`]: the external-specification
/// policy is [`ExternalSpecPolicy::Warn`], which emits byte-for-byte what
/// [`ExternalSpecPolicy::Ignore`] emits and additionally reports each linked
/// library whose own proof obligations the output does not carry. A caller that
/// has decided that report is uninteresting says so through
/// [`link_with_options`].
///
/// # Errors
///
/// The same conditions as [`link`].
pub fn link_with_warnings(
    main_wasm: &[u8],
    externals: &[(&str, &[u8])],
    contracts: Option<&[ImportWriteSet]>,
) -> Result<LinkOutput, LinkError> {
    link_with_options(main_wasm, externals, contracts, &LinkOptions::default())
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
/// of the output answer to is [`LinkError::AmbiguousObligationSymbol`]. Both are
/// checked here because the merge is the last phase that knows which import
/// supplied which body.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The default policy decides what every caller that never chose hears. It
    /// is `Warn` rather than `Ignore` because the two emit identical bytes, so
    /// defaulting to the loud one cannot change an artifact while defaulting to
    /// the quiet one would preserve the old silence for every such caller.
    #[test]
    fn link_options_default_is_warn() {
        assert_eq!(
            LinkOptions::default().external_specs,
            ExternalSpecPolicy::Warn
        );
        assert_eq!(ExternalSpecPolicy::default(), ExternalSpecPolicy::Warn);
    }

    /// The dropped-obligations report prescribes an opt-in, and the outcome it
    /// promises is not one the undecoded section can be held to: a library
    /// shipping only reachability obligations produces a follow-up build that
    /// adopts nothing. The sentence has to leave room for that rather than
    /// promise the obligations will arrive.
    #[test]
    fn the_dropped_report_does_not_promise_an_outcome_it_cannot_support() {
        let message = LinkWarning::ExternalSpecsDropped {
            modules: vec!["mathlib".to_string()],
        }
        .to_string();
        assert!(
            message.contains("--adopt-external-specs")
                && message.contains("[verification] adopt-external-specs = true"),
            "both spellings of the opt-in must stay in the report, got: {message}"
        );
        assert!(
            message.contains("reports every obligation it could not carry"),
            "the report must say the follow-up build accounts for what it cannot carry, rather \
             than promise the obligations arrive, got: {message}"
        );
    }

    /// A library whose every obligation is a reachability obligation mints no
    /// key at all, so the report about it must not close by saying the
    /// universal obligations were adopted: there were none, and the reader who
    /// asked for adoption would otherwise have to read the `.v` to find out.
    #[test]
    fn the_reachability_report_says_whether_anything_was_adopted() {
        let carried_nothing = LinkWarning::ReachabilityObligationsNotAdopted {
            module: "mathlib".to_string(),
            adopted: 0,
            obligations: vec!["`OnlyReach` / `OnlyReach.reaches_zero` (exists)".to_string()],
        }
        .to_string();
        assert!(
            carried_nothing.contains("carried nothing")
                && carried_nothing.contains("Nothing from this library reached the proof artifact"),
            "a link that adopted nothing must say so, got: {carried_nothing}"
        );
        assert!(
            !carried_nothing.contains("The universal obligations were adopted"),
            "the partial-success clause must not appear when nothing was adopted, got: \
             {carried_nothing}"
        );

        let carried_some = LinkWarning::ReachabilityObligationsNotAdopted {
            module: "mathlib".to_string(),
            adopted: 1,
            obligations: vec!["`Bounds` / `Bounds.reaches_zero` (exists)".to_string()],
        }
        .to_string();
        assert!(
            carried_some.contains("carried its universal (`forall`) obligations only")
                && carried_some.contains("The universal obligations were adopted and are \
                                          unaffected"),
            "a partial adoption keeps the clause that says the working half survived, got: \
             {carried_some}"
        );

        // Both renders describe the same left-behind obligation, so what differs
        // between them is the count and nothing else about the input.
        for message in [&carried_nothing, &carried_some] {
            assert!(
                message.contains("1 reachability obligation was left behind"),
                "every render names what it left behind, got: {message}"
            );
        }
    }
}
