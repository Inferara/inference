//! Whether a finished `.wasm` artifact is one its target's runtime can load.
//!
//! Every other crate in this workspace answers a question about a *program*:
//! what it means, what it lowers to, whether it links. This one answers a
//! question about the *bytes*, asked after linking and after every rewrite, of
//! whichever module is about to be written to disk. It therefore parses rather
//! than compiles, and its only dependencies are a WebAssembly decoder and an
//! error derive.
//!
//! One question here is also askable earlier. The `SpaceWasm` registration caps
//! bound what an *embedder* can be handed, not what a decoder reads, so they are
//! answerable of the imports a program declares before any bytes exist — and
//! answered there a refusal can name the `external fn` an author wrote. That
//! entry point sits beside the byte-level one and shares its body, so the two
//! vantages cannot give one import two answers; the byte-level check stays the
//! backstop, because it is the only one that sees an import linking dragged in.
//!
//! # Why the stock decoder
//!
//! The in-tree `inf-wasmparser` fork decodes and validates this compiler's
//! custom `0xfc` non-deterministic opcodes with no feature gate, which is what
//! makes it the right reader everywhere else in the pipeline and the wrong one
//! here: a decoder that accepts an instruction no standard names cannot testify
//! that a module is WebAssembly 1.0. Stock `wasmparser` can, and the version is
//! held at the workspace pin so the answer does not drift with an unrelated
//! upgrade.
//!
//! # One crate rather than one per caller
//!
//! Three programs ask these questions — `infc` before it writes an artifact,
//! `infs` before it replaces one with an optimized build, and the test suite
//! over the whole golden corpus. Spelled separately they would drift, and the
//! first symptom of the drift would be an artifact one of them accepted and a
//! flight computer refused. The crate is a true leaf — a decoder and an error
//! derive, nothing from this workspace — so linking it costs a caller nothing
//! but the decoder it would have needed anyway.
//!
//! # What is here
//!
//! - [`check_wasm1`] is the target-neutral question: does this module validate
//!   at the WebAssembly 1.0 feature set? Two targets are held to it, and a
//!   caller that knows where the bytes came from wraps the answer in a sentence
//!   that says so.
//! - [`spacewasm`] is the `SpaceWasm` flight interpreter's decode-time and
//!   registration-time envelope, plus the per-function measurements an embedder
//!   needs to size its two const generics — and the one refusal in the crate
//!   that is deliberately stricter than the decoder: a reference to a
//!   definition past the 16-bit IR word the interpreter narrows it into
//!   without checking, which loads and runs against a different definition. Its
//!   registration caps are also askable of the imports a program *declares*,
//!   before any bytes exist, so a build can name the `external fn` a developer
//!   wrote instead of an import section they did not.
//!
//! The refusal vocabulary is one enum, private to the crate and re-exported
//! from [`spacewasm`], which is the path a caller reaches it through. It
//! belongs to that target: every variant, every sentence rendered beneath one,
//! and the header above them all name one runtime.

#![warn(clippy::pedantic)]

mod errors;
pub mod spacewasm;

use wasmparser::{Validator, WasmFeatures};

/// The feature set both questions in this crate are asked at: the MVP plus
/// mutable globals.
///
/// Declared once because it is asked twice — [`check_wasm1`] of a whole
/// artifact or of one external, and [`spacewasm::check`] as the first of its
/// findings. Two constructions would answer the same question in two places,
/// and the first symptom of their drifting apart would be a per-external gate
/// and a post-link check disagreeing about the same bytes.
pub(crate) const WASM1_ENVELOPE: WasmFeatures = WasmFeatures::WASM1;

/// Validates `wasm` at the WebAssembly 1.0 feature set: the MVP plus mutable
/// globals, and nothing else.
///
/// This is the whole of what a target whose runtime predates every post-MVP
/// proposal will decode, and it is deliberately a feature question rather than
/// an instruction scan: a proposal grows the accepted set in more places than
/// its headline opcodes, and asking the validator is the only formulation that
/// stays right when the decoder learns a new one.
///
/// The answer is asked of whole artifacts and of their parts. A module linked
/// into a program has to clear the same bar as the program, and a caller
/// holding the artifacts separately can ask it of each one *before* merging
/// them — where a refusal can still name the file the offending instruction
/// came from. Asked only of the merged module, the answer is a byte offset into
/// bytes no file holds.
///
/// # Errors
///
/// Returns the validator's own message, which names the feature and the offset
/// within `wasm`. A caller that knows where these bytes came from is expected to
/// wrap it in a sentence that says so.
pub fn check_wasm1(wasm: &[u8]) -> Result<(), String> {
    Validator::new_with_features(WASM1_ENVELOPE)
        .validate_all(wasm)
        .map(|_| ())
        .map_err(|err| err.to_string())
}
