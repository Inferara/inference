//! `inf-llc` invocation for compiling LLVM IR to WebAssembly object files.
//!
//! This module handles invoking the `inf-llc` compiler — a modified LLVM `llc` with
//! support for Inference's custom non-deterministic intrinsics. It produces WebAssembly
//! object files (`.o`) from LLVM IR text (`.ll`).
//!
//! # Target-Aware Compilation
//!
//! The `inf-llc` invocation is configured based on the [`Target`] and [`OptLevel`]:
//!
//! | Setting | Wasm32 | Soroban |
//! |---------|--------|---------|
//! | `-mcpu` | `mvp` | `mvp` |
//! | `-mattr` | (none) | `+mutable-globals,+sign-ext,+bulk-memory` |
//! | Optimization | Target-dependent | Target-dependent |
//!
//! [`Target`]: inference_wasm_codegen::Target
//! [`OptLevel`]: inference_wasm_codegen::OptLevel

use std::path::Path;
use std::process::Command;

use inference_wasm_codegen::CodegenOutput;

use super::env::configure_llvm_env;
use super::paths::get_inf_llc_path;

/// Builds the argument list for `inf-llc` based on codegen output metadata.
///
/// This pure function extracts the argument-building logic from process spawning,
/// making it testable without requiring the `inf-llc` binary.
///
/// The returned arguments do NOT include the IR input path or `-o` output path,
/// which are appended separately during invocation.
fn build_llc_args(output: &CodegenOutput) -> Vec<String> {
    let target = output.target();
    let opt_level = output.opt_level();

    let mut args = vec![
        format!("-mcpu={}", target.cpu()),
        "-filetype=obj".to_string(),
        opt_level.as_llc_flag().to_string(),
    ];

    let features = target.llc_features();
    if !features.is_empty() {
        args.push(format!("-mattr={features}"));
    }

    args
}

/// Compiles LLVM IR to a WebAssembly object file using `inf-llc`.
///
/// Writes the IR from `output` to `ir_path`, invokes `inf-llc` with the appropriate
/// target-specific flags, and produces an object file at `obj_path`.
///
/// # Arguments
///
/// * `output` - The codegen output containing IR and target metadata
/// * `ir_path` - Path where the IR text file will be written
/// * `obj_path` - Path where the resulting object file will be written
///
/// # Errors
///
/// Returns an error if:
/// - The `inf-llc` binary cannot be found
/// - Writing the IR file fails
/// - The `inf-llc` process fails (non-zero exit status)
pub(crate) fn compile_ir_to_object(
    output: &CodegenOutput,
    ir_path: &Path,
    obj_path: &Path,
) -> anyhow::Result<()> {
    let llc_path = get_inf_llc_path()?;

    // Write IR to the temporary file
    output.write_ir_to(ir_path).map_err(|e| {
        anyhow::anyhow!("Failed to write LLVM IR to {}: {e}", ir_path.display())
    })?;

    let mut cmd = Command::new(&llc_path);
    configure_llvm_env(&mut cmd)?;

    for arg in build_llc_args(output) {
        cmd.arg(arg);
    }

    cmd.arg(ir_path).arg("-o").arg(obj_path);

    let result = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute {}: {e}", llc_path.display()))?;

    if !result.status.success() {
        return Err(anyhow::anyhow!(
            "inf-llc failed with status: {}\nstderr: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inference_wasm_codegen::{CodegenOutput, CompilationMode, OptLevel, Target};

    fn make_output(target: Target, opt_level: OptLevel) -> CodegenOutput {
        CodegenOutput::new(
            String::new(),
            target,
            CompilationMode::Compile,
            opt_level,
            "test".to_string(),
            false,
        )
    }

    #[test]
    fn llc_args_wasm32_o3() {
        let output = make_output(Target::Wasm32, OptLevel::O3);
        let args = build_llc_args(&output);

        assert!(args.contains(&"-mcpu=mvp".to_string()));
        assert!(args.contains(&"-filetype=obj".to_string()));
        assert!(args.contains(&"-O3".to_string()));
        // Wasm32 has no feature flags
        assert!(!args.iter().any(|a| a.starts_with("-mattr=")));
    }

    #[test]
    fn llc_args_soroban_oz() {
        let output = make_output(Target::Soroban, OptLevel::Oz);
        let args = build_llc_args(&output);

        assert!(args.contains(&"-mcpu=mvp".to_string()));
        assert!(args.contains(&"-filetype=obj".to_string()));
        // Oz maps to -O2 for llc
        assert!(args.contains(&"-O2".to_string()));
        assert!(args.contains(&"-mattr=+mutable-globals,+sign-ext,+bulk-memory".to_string()));
    }

    #[test]
    fn llc_args_wasm32_o0() {
        let output = make_output(Target::Wasm32, OptLevel::O0);
        let args = build_llc_args(&output);

        assert!(args.contains(&"-O0".to_string()));
    }

    #[test]
    fn llc_args_os_maps_to_o2() {
        let output = make_output(Target::Wasm32, OptLevel::Os);
        let args = build_llc_args(&output);

        // Os maps to -O2 for llc (size optimization is via IR attributes)
        assert!(args.contains(&"-O2".to_string()));
    }
}
