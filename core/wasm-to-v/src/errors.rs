//! Typed error variants for the WASM-to-Rocq translator.
//!
//! Public consumers receive `anyhow::Result<T>` from the crate's APIs; this
//! enum is the structured variant the translator wraps in `anyhow!(...)`.
//! It enables downcasting in the CLI so user-facing diagnostics can render
//! purpose-specific text (e.g., "rename your source file" for a Rocq stdlib
//! collision) instead of a generic "translation failed" line.

use thiserror::Error;

/// Why a candidate Rocq identifier was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum InvalidIdentifierReason {
    #[error("name is empty")]
    EmptyName,
    #[error("must start with a letter (A-Z or a-z); found `{0}`")]
    LeadingNonAlpha(char),
    #[error("contains invalid character `{0}`")]
    ContainsInvalidChar(char),
    #[error("contains `__` (reserved as the module/spec name separator)")]
    ContainsDoubleUnderscore,
    #[error("name exceeds the 255-character limit")]
    TooLong,
    #[error("collides with a Rocq reserved keyword")]
    ReservedKeyword,
}

/// Error variants surfaced by the WASM-to-Rocq translator.
///
/// Public type; marked `#[non_exhaustive]` so adding a new variant in a future
/// release is not a breaking change for downstream consumers that match on it.
/// Downcast via `anyhow::Error::downcast_ref::<WasmToVError>()` is unaffected
/// (downcasting matches type identity, not pattern-completeness).
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum WasmToVError {
    /// The candidate Rocq identifier (module or spec name) does not satisfy
    /// the validator's syntactic rules.
    #[error("invalid Rocq identifier `{name}`: {reason}")]
    InvalidRocqIdentifier {
        name: String,
        reason: InvalidIdentifierReason,
    },

    /// The candidate name is syntactically valid but would shadow a type
    /// auto-imported from the Rocq standard library.
    #[error("`{name}` would shadow a Rocq standard-library type")]
    RocqStdlibShadow { name: String },

    /// Joining the output module name and a spec name into the emitted
    /// `<module>__<spec>_specs` / `valid_<module>__<spec>` grammar (and its
    /// reachability siblings `__ex_specs` / `__uq_specs` /
    /// `valid_exists_<module>__<spec>` / `valid_unique_<module>__<spec>`,
    /// which contain the same join) would fabricate Rocq's reserved `__`
    /// separator. Each component is individually a legal identifier (no
    /// internal `__`), but a component that *ends* with `_` abuts the join
    /// separator — the module name abuts `__` (yielding `___`), the spec name
    /// abuts a trailing `_`-led suffix such as `_specs` (yielding `__`). The
    /// `__` run is reserved so the `<module>__<spec>` split stays unambiguous,
    /// so the name is rejected with a rename hint rather than auto-escaped:
    /// proof-mode names appear verbatim in the generated `.v`, and escaping
    /// would make them unreadable.
    #[error(
        "the {offender_kind} `{offender}` ends with `_`, so joining it into the Rocq proof name \
         `{joined}` fabricates the reserved `__` separator; rename it to `{fix_hint}`"
    )]
    SpecNameReservesSeparator {
        /// `output module name` (the entry file stem) or `spec`.
        offender_kind: String,
        /// The offending component as written.
        offender: String,
        /// The fabricated joined name (`main__Spec__specs`, `app___Foo`), shown so
        /// the user sees exactly what the join produces.
        joined: String,
        /// A concrete renamed form (the offender with its trailing `_` trimmed).
        fix_hint: String,
    },

    /// The output module name is reserved in the generated `.v`, so the module
    /// record cannot take it. Every generated `.v` names the record
    /// `Definition <module> : module` and judges it in `Theorem valid_<module>`,
    /// so the record contests every other binding of that name in scope there,
    /// or in a proof that imports the `.v`; [`ReservedNameOwner`] says which
    /// binding, and how the clash breaks the file or such a proof.
    ///
    /// The name is the caller's `mod_name`, unless the binary's `name` section
    /// carries a module name, which takes precedence (`codegen` writes its own
    /// `module_name` there) and is checked the same way. So the rejected name
    /// may be one the binary brought rather than the one the caller passed.
    ///
    /// Every owner shares one fix, which is why they share one variant. The
    /// module name is the one name in the file with nowhere to move to: it is
    /// the `.v`'s identity, the subject of the validity theorem, and the prefix
    /// of every spec-derived proof name. Rejected with a rename hint rather
    /// than auto-renamed, for the reason [`Self::SpecNameReservesSeparator`]
    /// is: proof-mode names appear verbatim in the `.v`, so quietly renaming
    /// the module would rename the artifact a downstream proof imports.
    #[error("{}", module_name_reserved_message(.name, .owner, .fix_hint))]
    ModuleNameReserved {
        /// The contested name, exactly as it would have been emitted.
        name: String,
        /// What the name is reserved for.
        owner: ReservedNameOwner,
        /// A concrete free name, so the fix needs no guessing.
        fix_hint: String,
    },

    /// `translate_bytes` was called with an explicit non-empty spec map and the
    /// WASM binary also embeds an `inference.spec_funcs` section, but the two
    /// disagree. We refuse to silently override either side.
    ///
    /// * `explicit` — map passed by the caller (CLI argument, tooling input).
    /// * `embedded` — map decoded from the WASM custom section.
    #[error("explicit spec map disagrees with the embedded `inference.spec_funcs` section")]
    EmbeddedSpecMismatch {
        /// Spec map supplied by the caller of `translate_bytes`.
        explicit: rustc_hash::FxHashMap<String, Vec<u32>>,
        /// Spec map decoded from the WASM `inference.spec_funcs` section.
        embedded: rustc_hash::FxHashMap<String, Vec<u32>>,
    },

    /// `translate_bytes` was called with an explicit non-empty hspec map and
    /// the WASM binary also embeds an `inference.hspecs` section, but the two
    /// disagree. The sibling of [`Self::EmbeddedSpecMismatch`] for the
    /// obligation payload; we refuse to silently override either side.
    ///
    /// * `explicit` — map passed by the caller (empty post-link, populated
    ///   pre-link for the cross-check).
    /// * `embedded` — map decoded from the WASM custom section.
    #[error("explicit hspec map disagrees with the embedded `inference.hspecs` section")]
    EmbeddedHspecsMismatch {
        /// Obligation map supplied by the caller of `translate_bytes`.
        explicit: inference_hassert::HSpecMap,
        /// Obligation map decoded from the WASM `inference.hspecs` section.
        embedded: inference_hassert::HSpecMap,
    },

    /// A hspec obligation names, or a spec-name set disagrees with, something
    /// the emitted module cannot represent: a spec name absent from
    /// `inference.spec_funcs`; a `T_app`/`HA_app_ok` symbol that resolves to
    /// zero, more than one, an imported, or a spec (omitted or retained)
    /// function; a reachability (`exists`/`unique`) obligation whose target
    /// cannot be resolved through the name section or whose frame metadata
    /// (entry arity, source-visible slots) does not fit the retained function;
    /// or an executable construct referencing a spec function. A corrupt or
    /// inconsistent proof artifact, rejected fail-closed.
    #[error("inconsistent `inference.hspecs` obligations: {0}")]
    HspecInconsistent(String),

    /// A WASM parser error surfaced during the parse phase.
    #[error("WASM parse error: {0}")]
    WasmParse(String),

    /// An otherwise valid WASM construct uses a feature the translator does
    /// not yet support (reference types, multi-memory, atomics, GC, etc.).
    /// Distinguished from `WasmParse` so callers can render "not yet
    /// supported" guidance rather than "malformed binary".
    #[error("unsupported WASM feature: {description}")]
    UnsupportedFeature { description: String },
}

/// What a name the output module cannot take is reserved for, carried by
/// [`WasmToVError::ModuleNameReserved`].
///
/// Each owner contests the module record in its own way, so each gets its own
/// explanation, but the fix is the same for all of them: rename the source file
/// the module name comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReservedNameOwner {
    /// One of the helper `Definition`s every generated `.v` opens with. The
    /// record would define the same name a second time, and Rocq definitions
    /// are not overloadable, so `coqc` rejects the whole file.
    PreambleHelper,
    /// A name the proof contract declares, listed in
    /// [`crate::rocq_names::ROCQ_CONTRACT_NAMES`]. Defining it is legal, but
    /// wherever the declaring library is imported first, the record shadows
    /// the library's name: in a proof that imports the `.v` after the library,
    /// and in the `.v` itself wherever it spells the name again. Code written
    /// for the library's name then gets the record instead. That is usually an
    /// error, but not always: a shadowed nullary constructor in a pattern
    /// silently becomes a pattern variable that matches anything.
    Contract {
        /// The logical path of the declaring library, such as `Wasm.host`.
        library: &'static str,
    },
    /// The `ho` binder of the host instance, which every generated `.v` binds
    /// in the `Section Host` its theorems are stated under. Inside that
    /// section the name resolves to the host instance, which is not a module,
    /// so every theorem naming the record there fails to type-check.
    HostSectionBinder,
}

/// The one-line description of a [`WasmToVError::ModuleNameReserved`], for its
/// owner.
fn module_name_reserved_message(name: &str, owner: &ReservedNameOwner, fix_hint: &str) -> String {
    match owner {
        ReservedNameOwner::PreambleHelper => format!(
            "the output module name `{name}` is one of the helper definitions the emitted Rocq \
             preamble always occupies, so `{name}` would name two top-level definitions in one \
             file; rename it to `{fix_hint}`"
        ),
        ReservedNameOwner::Contract { library } => {
            let imported = if owner.is_imported_conditionally() {
                "the generated Rocq file imports whenever the module carries a reachability \
                 obligation (the name is reserved either way)"
            } else {
                "every generated Rocq file imports"
            };
            format!(
                "the output module name `{name}` is declared by `{library}`, a proof-contract \
                 library {imported}, so the module record would shadow the library's `{name}`; \
                 rename the module to `{fix_hint}`"
            )
        }
        ReservedNameOwner::HostSectionBinder => format!(
            "the output module name `{name}` is the name the generated Rocq file gives its host \
             instance, so the file's theorems would read it as the host instance instead of the \
             module record; rename it to `{fix_hint}`"
        ),
    }
}
