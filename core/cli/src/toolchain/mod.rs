//! Toolchain module for compiling LLVM IR to WebAssembly via external tools.
//!
//! This module orchestrates the two-stage external compilation pipeline:
//!
//! 1. **`inf-llc`** — compiles LLVM IR (`.ll`) to a WebAssembly object file (`.o`)
//! 2. **`rust-lld`** — links the object file into a final WebAssembly module (`.wasm`)
//!
//! The [`compile_ir_to_wasm`] function is the main entry point. It reads target and
//! mode information from [`CodegenOutput`] and applies the correct flags for each tool.
//!
//! # Architecture
//!
//! ```text
//! CodegenOutput (IR + metadata)
//!         |
//!         v
//!   compile_ir_to_wasm()
//!         |
//!         +---> llc::compile_ir_to_object()   [inf-llc]
//!         |           |
//!         |           v
//!         +---> lld::link_object_to_wasm()    [rust-lld]
//!         |           |
//!         |           v
//!         +---> Read .wasm bytes, clean up
//!         |
//!         v
//!     Vec<u8>  (WASM binary)
//! ```
//!
//! # Target-Aware Compilation
//!
//! The toolchain reads [`Target`] from the `CodegenOutput` and configures flags:
//!
//! - **Wasm32**: Strict MVP, no features, `--export=main` if applicable
//! - **Soroban**: Post-MVP features, size optimization, export-dynamic, gc-sections
//!
//! [`CodegenOutput`]: inference_wasm_codegen::CodegenOutput
//! [`Target`]: inference_wasm_codegen::Target

mod env;
mod llc;
mod lld;
pub(super) mod paths;
pub mod profile;

pub use profile::BuildProfile;

use inference_wasm_codegen::CodegenOutput;
use tempfile::tempdir;

/// Compiles LLVM IR to WebAssembly bytes.
///
/// This function orchestrates the full compilation pipeline:
///
/// 1. Creates a temporary directory for intermediate files
/// 2. Writes the IR to a `.ll` file
/// 3. Invokes `inf-llc` to produce a `.o` object file
/// 4. Invokes `rust-lld` to link the object into a `.wasm` module
/// 5. Reads the `.wasm` bytes and cleans up temporary files
///
/// Target-specific flags (CPU features, linker options, optimization level)
/// are derived from the `output`'s [`Target`] and [`CompilationMode`].
///
/// # Arguments
///
/// * `output` - The codegen output containing LLVM IR and compilation metadata
///
/// # Errors
///
/// Returns an error if:
/// - Temporary directory creation fails
/// - `inf-llc` or `rust-lld` binaries are not found
/// - Compilation or linking fails
/// - File I/O operations fail
///
/// [`Target`]: inference_wasm_codegen::Target
/// [`CompilationMode`]: inference_wasm_codegen::CompilationMode
pub fn compile_ir_to_wasm(output: &CodegenOutput) -> anyhow::Result<Vec<u8>> {
    let temp_dir = tempdir()?;
    let module_name = output.module_name();

    let ir_path = temp_dir.path().join(module_name).with_extension("ll");
    let obj_path = temp_dir.path().join(module_name).with_extension("o");
    let wasm_path = temp_dir.path().join(module_name).with_extension("wasm");

    // Stage 1: Compile IR to object file
    llc::compile_ir_to_object(output, &ir_path, &obj_path)?;

    // Stage 2: Link object file to WASM module
    lld::link_object_to_wasm(output, &obj_path, &wasm_path)?;

    // Stage 3: Read WASM bytes
    let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read compiled WASM from {}: {e}",
            wasm_path.display()
        )
    })?;

    // Temp directory is cleaned up automatically when `temp_dir` is dropped
    Ok(wasm_bytes)
}
