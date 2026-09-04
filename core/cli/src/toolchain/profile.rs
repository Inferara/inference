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
//! | Profile | Wasm32 Compile | Soroban Compile | Proof (any target) |
//! |---------|----------------|-----------------|---------------------|
//! | Debug   | O0             | O0              | O3 / Oz             |
//! | Release | O3             | Oz              | O3 / Oz             |
//!
//! `Release` is the default, matching the current behavior where Wasm32 Compile
//! records O3. In Proof mode, build profiles are ignored -- the target's
//! release-profile level is always recorded, regardless of `self`.

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
    /// Records the target-appropriate level: Wasm32 gets `-O3`, Soroban gets
    /// `-Oz`.
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
                Self::Release => match target {
                    Target::Wasm32 => OptLevel::O3,
                    Target::Soroban => OptLevel::Oz,
                },
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

    // --- Release profile ---

    #[test]
    fn release_wasm32_compile_is_o3() {
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Wasm32, CompilationMode::Compile),
            OptLevel::O3,
        );
    }

    #[test]
    fn release_soroban_compile_is_oz() {
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Soroban, CompilationMode::Compile),
            OptLevel::Oz,
        );
    }

    #[test]
    fn release_wasm32_proof_is_o3() {
        // Proof mode always records the target's release-profile level.
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O3,
        );
    }

    #[test]
    fn release_soroban_proof_is_oz() {
        // Proof mode always records the target's release-profile level.
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Soroban, CompilationMode::Proof),
            OptLevel::Oz,
        );
    }

    // --- Debug profile ---

    #[test]
    fn debug_wasm32_compile_is_o0() {
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Wasm32, CompilationMode::Compile),
            OptLevel::O0,
        );
    }

    #[test]
    fn debug_soroban_compile_is_o0() {
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Soroban, CompilationMode::Compile),
            OptLevel::O0,
        );
    }

    #[test]
    fn debug_wasm32_proof_is_o3() {
        // Proof mode ignores profile, always records the target's release-profile level.
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O3,
        );
    }

    #[test]
    fn debug_soroban_proof_is_oz() {
        // Proof mode ignores profile, always records the target's release-profile level.
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Soroban, CompilationMode::Proof),
            OptLevel::Oz,
        );
    }
}
