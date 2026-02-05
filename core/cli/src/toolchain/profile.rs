//! Build profile configuration for the Inference compiler.
//!
//! The [`BuildProfile`] enum controls optimization behavior for compilation.
//! It lives in the toolchain layer because it determines how external tools
//! (`inf-llc`, `rust-lld`) are configured.
//!
//! # Profile Matrix
//!
//! | Profile | Wasm32 Compile | Soroban Compile | Proof (any target) |
//! |---------|----------------|-----------------|---------------------|
//! | Debug   | O0             | O0              | O0                  |
//! | Release | O3             | Oz              | O0                  |
//!
//! `Release` is the default, matching the current behavior where Wasm32 Compile
//! uses `-O3`.

use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

/// Build profile controlling optimization level for compilation.
///
/// `Release` is the default, matching the existing behavior. `Debug` disables
/// optimization for faster builds and easier debugging.
///
/// In Proof mode, the profile is ignored — optimization is always `O0` to
/// preserve 1:1 structural correspondence for Rocq formalization.
///
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BuildProfile {
    /// No optimization (`-O0`). Faster builds, easier debugging.
    ///
    /// Not yet exposed via CLI flags — will be activated by `--debug` flag
    /// in a future issue.
    #[allow(dead_code)]
    Debug,
    /// Target-appropriate optimization. Wasm32 uses `-O3`, Soroban uses `-Oz`.
    #[default]
    Release,
}

impl BuildProfile {
    /// Resolves the optimization level for the given target and mode.
    ///
    /// In Proof mode, always returns `O0` regardless of profile — structural
    /// fidelity for Rocq formalization takes precedence over optimization.
    ///
    #[must_use]
    pub fn resolve_opt_level(self, target: Target, mode: CompilationMode) -> OptLevel {
        match mode {
            CompilationMode::Proof => OptLevel::O0,
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
    fn release_wasm32_proof_is_o0() {
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O0,
        );
    }

    #[test]
    fn release_soroban_proof_is_o0() {
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Soroban, CompilationMode::Proof),
            OptLevel::O0,
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
    fn debug_wasm32_proof_is_o0() {
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O0,
        );
    }

    #[test]
    fn debug_soroban_proof_is_o0() {
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Soroban, CompilationMode::Proof),
            OptLevel::O0,
        );
    }
}
