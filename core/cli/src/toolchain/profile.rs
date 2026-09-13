//! Build profile configuration for the Inference compiler.
//!
//! The [`BuildProfile`] enum selects which [`OptLevel`] is recorded on the
//! compiled output. Codegen applies no optimization pass over that value (see
//! [`OptLevel`]'s own documentation), so today `Debug` and `Release` produce
//! byte-identical WASM; the level exists to be meaningful once a downstream
//! tool -- a `[build.wasm-opt]` post-build step, say -- consumes it.
//!
//! # Profile Matrix
//!
//! | Profile | Compile                        | Proof                          |
//! |---------|--------------------------------|--------------------------------|
//! | Debug   | `O0`                           | `Target::default_opt_level()`  |
//! | Release | `Target::default_opt_level()`  | `Target::default_opt_level()`  |
//!
//! Three of the four cells are the same call and only one is a constant, so the
//! matrix names [`Target::default_opt_level`] rather than restating its rows. A
//! column per target is a table whose shape does not scale and a second place
//! for the per-target levels to be wrong; where the levels are decided is
//! `inference-wasm-codegen`'s `Target`, and this module's job is which of the
//! two answers a profile picks.
//!
//! `Release` is the default. In Proof mode the profile is ignored -- the
//! target's default level is always recorded, so the level on a proof artifact
//! is the one a deployed artifact would carry.

use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

/// Build profile selecting which [`OptLevel`] is recorded for compilation.
///
/// `Release` is the default, matching the existing behavior. Codegen applies
/// no optimization pass over the recorded level, so `Debug` and `Release`
/// currently produce byte-identical WASM; the distinction is preserved for
/// a downstream tool that consumes the level.
///
/// In Proof mode, the profile is ignored -- the target's release-profile
/// level is always recorded.
///
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildProfile {
    /// Records `-O0`. Not yet exposed via CLI flags — will be activated by
    /// `--debug` flag in a future issue.
    #[allow(dead_code)]
    Debug,
    /// Records the target's own [`Target::default_opt_level`].
    #[default]
    Release,
}

impl BuildProfile {
    /// Resolves the [`OptLevel`] to record for the given target and mode.
    ///
    /// In Proof mode, returns the target's release-profile level regardless
    /// of `self`, so the recorded level always matches what a deployed
    /// artifact would carry rather than a debug one.
    ///
    #[must_use]
    pub fn resolve_opt_level(self, target: Target, mode: CompilationMode) -> OptLevel {
        match mode {
            // Proof mode always records the target's release-profile level.
            CompilationMode::Proof => target.default_opt_level(),
            CompilationMode::Compile => match self {
                Self::Debug => OptLevel::O0,
                Self::Release => target.default_opt_level(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_release() {
        assert_eq!(BuildProfile::default(), BuildProfile::Release);
    }

    /// The whole matrix, for every target that exists rather than for the two
    /// somebody thought to write down.
    ///
    /// Three of the four cells must be the target's own default level and the
    /// fourth `O0`; nothing else in the tree performs this cross-check, so
    /// before it a new target reached `resolve_opt_level` through whichever arm
    /// a `match` happened to give it. It fails if a cell is rewritten to a
    /// literal that disagrees with the target, and it covers a new target the
    /// day it joins `Target::ALL`.
    ///
    /// Those three assertions are written in terms of the same call the
    /// implementation makes, so what they pin is which of the two answers a
    /// profile picks, not the levels themselves: rewriting a target's
    /// `default_opt_level` moves both sides and leaves them green. The literal
    /// table below is what stops that -- one non-self-referential row per
    /// target, so a wrong per-target default fails here and not only in
    /// `inference-wasm-codegen`, where each level is also pinned literally.
    ///
    /// The table is written out rather than derived, and a new target has to be
    /// added to it by hand. The loop is what covers `Target::ALL` on the day a
    /// variant joins it; the table is what a reader compares against the release
    /// notes.
    #[test]
    fn every_target_resolves_all_four_cells_through_its_own_default() {
        for (target, level) in [
            (Target::Wasm32, OptLevel::O3),
            (Target::Stellar, OptLevel::Oz),
            (Target::SpaceWasm, OptLevel::Os),
        ] {
            assert_eq!(
                BuildProfile::Release.resolve_opt_level(target, CompilationMode::Compile),
                level,
                "`{}`'s release compile level is the one this module ships with",
                target.as_str()
            );
        }

        for target in Target::ALL {
            let default = target.default_opt_level();
            assert_eq!(
                BuildProfile::Release.resolve_opt_level(target, CompilationMode::Compile),
                default,
                "release compile must record `{}`'s own default",
                target.as_str()
            );
            assert_eq!(
                BuildProfile::Debug.resolve_opt_level(target, CompilationMode::Compile),
                OptLevel::O0,
                "debug compile records O0 at every target"
            );
            assert_eq!(
                BuildProfile::Release.resolve_opt_level(target, CompilationMode::Proof),
                default,
                "proof mode ignores the profile"
            );
            assert_eq!(
                BuildProfile::Debug.resolve_opt_level(target, CompilationMode::Proof),
                default,
                "proof mode ignores the profile"
            );
        }
    }
}
