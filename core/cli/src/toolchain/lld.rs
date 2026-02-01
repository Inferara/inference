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

    cmd.arg("-flavor")
        .arg("wasm")
        .arg(obj_path)
        .arg("--no-entry");

    match output.target() {
        Target::Wasm32 => {
            if output.has_main() {
                cmd.arg("--export=main");
            }
        }
        Target::Soroban => {
            cmd.arg("--export-dynamic")
                .arg("--gc-sections")
                .arg("-z")
                .arg("stack-size=1048576")
                .arg("--stack-first");
        }
    }

    cmd.arg("-o").arg(wasm_path);

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
