//! Consolidated error types for the compiler-interface crate.
//!
//! Every rejection of a build setting a user selected is one of these, and the
//! `#[error(...)]` renderings are the single source of the wording: `infs`
//! (validating `Inference.toml`) and `infc` (validating its flags) both surface
//! a `Display`, so the same mistake reads identically whichever front end
//! catches it. Each error carries the surface the setting was written on, so the
//! message can name the exact thing the user has to edit.

use thiserror::Error;

use crate::{
    MemoryLayoutSource, RESERVED_TARGET_NAMES, TargetName, TargetSource, WasmFeatureName,
    WasmFeatureSource, supported_features_listing, supported_targets_listing,
};

/// A requested WebAssembly feature set that cannot be honored.
///
/// The `surface` field is deliberately not named `source`: `thiserror` reserves
/// that name for a wrapped causal error, which this is not.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WasmFeatureError {
    /// A name that is not in the requestable vocabulary.
    #[error(
        "Invalid {} entry `{entry}`: unknown WebAssembly feature. Supported features: {}.{}",
        surface.label(),
        supported_features_listing(),
        whitespace_hint(entry)
    )]
    UnknownFeature {
        entry: String,
        surface: WasmFeatureSource,
    },

    /// An instruction name written where a proposal name belongs.
    #[error(
        "Invalid {} entry `{entry}`: `{entry}` is an instruction, not a feature. \
         Features are named after the proposal that introduced them, which enables the \
         whole instruction family at once — write `{}` instead.",
        surface.label(),
        proposal.as_str()
    )]
    InstructionName {
        entry: String,
        proposal: WasmFeatureName,
        surface: WasmFeatureSource,
    },

    /// A feature that is always on, and so cannot be requested.
    #[error(
        "Invalid {} entry `{entry}`: `{entry}` is always enabled and cannot be requested. \
         Every Inference module that allocates a stack frame uses a mutable `__stack_pointer` \
         global, so this is part of the baseline rather than an opt-in — remove the entry.",
        surface.label()
    )]
    InherentFeature {
        entry: String,
        surface: WasmFeatureSource,
    },

    /// A feature listed more than once.
    #[error(
        "Invalid {}: `{entry}` is listed more than once. Each feature may appear at most \
         once — remove the duplicate.",
        surface.label()
    )]
    DuplicateFeature {
        entry: String,
        surface: WasmFeatureSource,
    },
}

/// A named compilation target that cannot be honored.
///
/// The `surface` field is deliberately not named `source`, for the reason given
/// on [`WasmFeatureError`]: `thiserror` reserves that name for a wrapped causal
/// error, which a [`TargetSource`] is not.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TargetError {
    /// A name that is not in the requestable vocabulary.
    #[error(
        "Invalid {} value `{entry}`: unknown compilation target. Supported targets: {}.{}",
        surface.label(),
        supported_targets_listing(),
        target_whitespace_hint(entry)
    )]
    UnknownTarget {
        entry: String,
        surface: TargetSource,
    },

    /// A former name of a target that is now requestable under a different
    /// spelling. See [`RESERVED_TARGET_NAMES`] for the obligation this wording
    /// puts on anything added to that slice.
    #[error(
        "Invalid {} value `{entry}`: `{entry}` is the former name of the `stellar` target and \
         is not accepted; write `stellar` instead. Supported targets: {}.",
        surface.label(),
        supported_targets_listing()
    )]
    ReservedTarget {
        entry: String,
        surface: TargetSource,
    },
}

/// A requested linear memory that cannot be honored.
///
/// A struct rather than an enum because there is exactly one way a layout
/// request fails — the two numbers do not describe a memory a module can
/// declare — and the invariant that was broken is already spelled out in
/// `reason`. Splitting that into variants would duplicate the rules in a second
/// place without telling a caller anything the message does not.
///
/// `surface` is deliberately not named `source`: `thiserror` reserves that name
/// for a wrapped causal error, which this is not.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Invalid {}: {reason}", surface.label())]
pub struct MemoryLayoutError {
    /// The violated invariant, phrased as an explanation of the numbers the
    /// build asked for and naming the offending one.
    pub reason: String,
    pub surface: MemoryLayoutSource,
}

/// The extra sentence an unknown name earns when it is a supported name with
/// whitespace around it.
///
/// Whitespace is rejected, never trimmed — but a trailing space inside a TOML
/// string is invisible in the echoed entry, so the message has to name the cause
/// or the user sees their own spelling reported back as unknown.
fn whitespace_hint(entry: &str) -> String {
    if entry.trim() != entry && WasmFeatureName::from_name(entry.trim()).is_some() {
        format!(
            " Feature names are matched exactly and this entry has surrounding whitespace: \
             write `{}`.",
            entry.trim()
        )
    } else {
        String::new()
    }
}

/// The extra sentence an unknown target earns when it is a supported target with
/// whitespace around it, or a reserved name with whitespace around it.
///
/// The reserved half matters as much as the supported one: without it a
/// `"soroban "` written in a manifest is reported as an unknown name, which is
/// the message the reserved sentence exists to avoid, and the user cannot see
/// the space that caused it. Mirrors [`whitespace_hint`] for the same reason it
/// exists — whitespace is rejected, never trimmed.
fn target_whitespace_hint(entry: &str) -> String {
    let trimmed = entry.trim();
    let recognized =
        TargetName::from_name(trimmed).is_some() || RESERVED_TARGET_NAMES.contains(&trimmed);
    if trimmed != entry && recognized {
        format!(
            " Target names are matched exactly and this entry has surrounding whitespace: \
             write `{trimmed}`."
        )
    } else {
        String::new()
    }
}
