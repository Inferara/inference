//! `rust-lld` invocation for linking WebAssembly object files into WASM modules.
//!
//! This module handles invoking `rust-lld` (the WebAssembly linker from the Rust
//! toolchain) to link `.o` object files produced by `inf-llc` into final `.wasm`
//! modules.
//!
//! # Target-Aware Linking
//!
//! Linker flags differ by target:
//!
//! | Flag | Wasm32 | Soroban |
//! |------|--------|---------|
//! | `--no-entry` | Yes | Yes |
//! | `--export=main` | If `has_main` | No |
//! | `--export-dynamic` | No | Yes |
//! | `--gc-sections` | No | Yes |
//! | `-z stack-size=1048576` | No | Yes |
//! | `--stack-first` | No | Yes |

use std::path::Path;
use std::process::Command;

use inference_wasm_codegen::{CodegenOutput, Target};

use super::env::configure_llvm_env;
use super::paths::get_rust_lld_path;

/// Builds the argument list for `rust-lld` based on codegen output metadata.
///
/// This pure function extracts the argument-building logic from process spawning,
/// making it testable without requiring the `rust-lld` binary.
///
/// The returned arguments include `-flavor wasm`, `--no-entry`, and all
/// target-specific flags. The object input path and `-o` output path are NOT
/// included and must be appended separately during invocation.
fn build_lld_args(output: &CodegenOutput) -> Vec<String> {
    let mut args = vec![
        "-flavor".to_string(),
        "wasm".to_string(),
        "--no-entry".to_string(),
    ];

    match output.target() {
        Target::Wasm32 => {
            if output.has_main() {
                args.push("--export=main".to_string());
            }
        }
        Target::Soroban => {
            args.push("--export-dynamic".to_string());
            args.push("--gc-sections".to_string());
            args.push("-z".to_string());
            args.push("stack-size=1048576".to_string());
            args.push("--stack-first".to_string());
        }
    }

    args
}

/// Links a WebAssembly object file into a final WASM module using `rust-lld`.
///
/// Invokes `rust-lld` with target-specific flags to produce a `.wasm` file from
/// the object file at `obj_path`.
///
/// # Arguments
///
/// * `output` - The codegen output containing target and `has_main` metadata
/// * `obj_path` - Path to the WebAssembly object file (produced by `inf-llc`)
/// * `wasm_path` - Path where the final `.wasm` module will be written
///
/// # Errors
///
/// Returns an error if:
/// - The `rust-lld` binary cannot be found
/// - The linking process fails (non-zero exit status)
pub(crate) fn link_object_to_wasm(
    output: &CodegenOutput,
    obj_path: &Path,
    wasm_path: &Path,
) -> anyhow::Result<()> {
    let lld_path = get_rust_lld_path()?;

    let mut cmd = Command::new(&lld_path);
    configure_llvm_env(&mut cmd)?;

    for arg in build_lld_args(output) {
        cmd.arg(arg);
    }

    cmd.arg(obj_path).arg("-o").arg(wasm_path);

    let result = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute {}: {e}", lld_path.display()))?;

    if !result.status.success() {
        return Err(anyhow::anyhow!(
            "rust-lld failed with status: {}\nstderr: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_wasm_codegen::{CodegenOutput, CompilationMode, Target};

    fn make_output(target: Target, has_main: bool) -> CodegenOutput {
        CodegenOutput::new(
            String::new(),
            target,
            CompilationMode::Compile,
            target.default_opt_level(),
            "test".to_string(),
            has_main,
        )
    }

    #[test]
    fn lld_args_wasm32_with_main() {
        let output = make_output(Target::Wasm32, true);
        let args = build_lld_args(&output);

        assert!(args.contains(&"-flavor".to_string()));
        assert!(args.contains(&"wasm".to_string()));
        assert!(args.contains(&"--no-entry".to_string()));
        assert!(args.contains(&"--export=main".to_string()));
        // Should NOT have Soroban-specific flags
        assert!(!args.contains(&"--export-dynamic".to_string()));
        assert!(!args.contains(&"--gc-sections".to_string()));
    }

    #[test]
    fn lld_args_wasm32_without_main() {
        let output = make_output(Target::Wasm32, false);
        let args = build_lld_args(&output);

        assert!(args.contains(&"-flavor".to_string()));
        assert!(args.contains(&"wasm".to_string()));
        assert!(args.contains(&"--no-entry".to_string()));
        assert!(!args.contains(&"--export=main".to_string()));
    }

    #[test]
    fn lld_args_soroban() {
        let output = make_output(Target::Soroban, false);
        let args = build_lld_args(&output);

        assert!(args.contains(&"-flavor".to_string()));
        assert!(args.contains(&"wasm".to_string()));
        assert!(args.contains(&"--no-entry".to_string()));
        assert!(args.contains(&"--export-dynamic".to_string()));
        assert!(args.contains(&"--gc-sections".to_string()));
        assert!(args.contains(&"-z".to_string()));
        assert!(args.contains(&"stack-size=1048576".to_string()));
        assert!(args.contains(&"--stack-first".to_string()));
        // Soroban should NOT have --export=main
        assert!(!args.contains(&"--export=main".to_string()));
    }
}
