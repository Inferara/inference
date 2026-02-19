//! Build profile configuration for the Inference compiler.
//!
//! The [`BuildProfile`] enum controls optimization behavior for compilation.
//! It determines the optimization level used during code generation.
//!
//! # Profile Matrix
//!
//! | Profile | Wasm32 Compile | Soroban Compile | Proof (any target) |
//! |---------|----------------|-----------------|---------------------|
//! | Debug   | O0             | O0              | O3 / Oz             |
//! | Release | O3             | Oz              | O3 / Oz             |
//!
//! `Release` is the default, matching the current behavior where Wasm32 Compile
//! uses O3. In Proof mode, build profiles are ignored -- the target's release
//! optimization is always used.

use inference_wasm_codegen::{CompilationMode, OptLevel, Target};

/// Build profile controlling optimization level for compilation.
///
/// `Release` is the default, matching the existing behavior. `Debug` disables
/// optimization for faster builds and easier debugging.
///
/// In Proof mode, the profile is ignored -- the target's release optimization
/// is always used.
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
    /// In Proof mode, returns the target's release optimization regardless of
    /// profile. This ensures Rocq proofs cover the deployed code.
    ///
    #[must_use]
    pub fn resolve_opt_level(self, target: Target, mode: CompilationMode) -> OptLevel {
        match mode {
            // Decision #32: Proof mode uses the target's release optimization.
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
        // Decision #32: Proof mode uses target's release optimization.
        assert_eq!(
            BuildProfile::Release.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O3,
        );
    }

    #[test]
    fn release_soroban_proof_is_oz() {
        // Decision #32: Proof mode uses target's release optimization.
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
        // Decision #32: Proof mode ignores profile, uses target's release optimization.
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Wasm32, CompilationMode::Proof),
            OptLevel::O3,
        );
    }

    #[test]
    fn debug_soroban_proof_is_oz() {
        // Decision #32: Proof mode ignores profile, uses target's release optimization.
        assert_eq!(
            BuildProfile::Debug.resolve_opt_level(Target::Soroban, CompilationMode::Proof),
            OptLevel::Oz,
        );
    }
}
