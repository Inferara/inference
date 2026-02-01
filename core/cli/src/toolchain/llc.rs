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

    // Determine optimization level from target and mode
    let target = output.target();
    let opt_level = target.default_opt_level(output.mode());

    let mut cmd = Command::new(&llc_path);
    configure_llvm_env(&mut cmd)?;

    cmd.arg(format!("-mcpu={}", target.cpu()))
        .arg("-filetype=obj")
        .arg(opt_level.as_llc_flag())
        .arg(ir_path)
        .arg("-o")
        .arg(obj_path);

    // Add feature flags if the target has any
    let features = target.llc_features();
    if !features.is_empty() {
        cmd.arg(format!("-mattr={features}"));
    }

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
