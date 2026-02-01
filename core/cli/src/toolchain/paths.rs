//! Binary discovery for external toolchain components.
//!
//! This module locates the `inf-llc` and `rust-lld` binaries required for
//! compiling LLVM IR to WebAssembly. Binaries are searched in platform-specific
//! locations relative to the current executable.
//!
//! # Search Strategy
//!
//! 1. Build-time hint via `INF_WASM_CODEGEN_BIN_DIR` environment variable
//! 2. `<executable-dir>/bin/<binary>` — regular build layout
//! 3. `<executable-dir>/../bin/<binary>` — test builds (executable in `deps/`)

use std::path::{Path, PathBuf};

/// Locates the `inf-llc` binary required for compilation.
///
/// `inf-llc` is a modified LLVM compiler with support for Inference's custom
/// non-deterministic intrinsics. It processes LLVM IR and produces WebAssembly
/// object files.
///
/// # Errors
///
/// Returns an error if `inf-llc` is not found in any of the expected locations.
pub(crate) fn get_inf_llc_path() -> anyhow::Result<PathBuf> {
    get_bin_path(
        "inf-llc",
        "This package requires LLVM with Inference intrinsics support.",
    )
}

/// Locates the `rust-lld` binary required for linking.
///
/// `rust-lld` is the WebAssembly linker from the Rust toolchain, invoked with
/// `-flavor wasm` to produce final `.wasm` modules from object files.
///
/// # Errors
///
/// Returns an error if `rust-lld` is not found in any of the expected locations.
pub(crate) fn get_rust_lld_path() -> anyhow::Result<PathBuf> {
    get_bin_path(
        "rust-lld",
        "This package requires rust-lld to link WebAssembly modules.",
    )
}

/// Returns the directory containing the current executable.
///
/// This is a shared helper used by both binary discovery ([`get_bin_path`]) and
/// LLVM library discovery ([`super::env::get_llvm_lib_dir`]).
///
/// # Errors
///
/// Returns an error if the current executable path cannot be determined.
pub(super) fn exe_dir() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

    exe_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))
}

/// Generic binary path resolver with multiple search strategies.
///
/// Searches for a binary in the following order:
/// 1. Build-time hint from `INF_WASM_CODEGEN_BIN_DIR` environment variable
/// 2. `<exe_dir>/bin/<binary>` — standard build layout
/// 3. `<exe_dir>/../bin/<binary>` — test builds where the executable is in `deps/`
///
/// Handles platform-specific executable suffixes (`.exe` on Windows).
///
/// # Parameters
///
/// - `bin_name` - Name of the binary without extension (e.g., `"inf-llc"`)
/// - `not_found_message` - Descriptive error message shown if the binary is not found
///
/// # Errors
///
/// Returns a detailed error listing all searched locations if the binary is not found.
fn get_bin_path(bin_name: &str, not_found_message: &str) -> anyhow::Result<PathBuf> {
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let bin_file_name = format!("{bin_name}{exe_suffix}");

    // First, try the build-time hint if available
    if let Some(bin_dir) = option_env!("INF_WASM_CODEGEN_BIN_DIR") {
        let candidate = PathBuf::from(bin_dir).join(&bin_file_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let dir = exe_dir()?;

    // Try multiple possible locations:
    // 1. For regular binaries: <exe_dir>/bin/<binary>
    // 2. For test binaries in deps/: <exe_dir>/../bin/<binary>
    let candidates = [
        dir.join("bin").join(&bin_file_name),
        dir.parent().map_or_else(
            || dir.join("bin").join(&bin_file_name),
            |p| p.join("bin").join(&bin_file_name),
        ),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "{bin_name} binary not found\n\
            \n\
            {not_found_message}\n\n\
            Executable directory: {}\n\
            Searched locations:\n  - {}\n  - {}",
        dir.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}
