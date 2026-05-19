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
#[derive(Debug, Clone, Error)]
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

    /// `translate_bytes` was called with an explicit non-empty spec map and the
    /// WASM binary also embeds an `inference.spec_funcs` section, but the two
    /// disagree. We refuse to silently override either side.
    #[error("explicit spec map disagrees with the embedded `inference.spec_funcs` section")]
    EmbeddedSpecMismatch {
        explicit: rustc_hash::FxHashMap<String, Vec<u32>>,
        embedded: rustc_hash::FxHashMap<String, Vec<u32>>,
    },

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
