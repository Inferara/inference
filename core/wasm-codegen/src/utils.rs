//! Utility functions for WebAssembly compilation via external LLVM toolchain.
//!
//! This module handles the invocation of external compilation tools (inf-llc and rust-lld)
//! to transform LLVM IR into WebAssembly bytecode. It manages temporary file creation,
//! toolchain location, and platform-specific environment configuration.
//!
//! # External Dependencies
//!
//! The compilation process requires two external binaries:
//!
//! - **inf-llc** - Modified LLVM compiler with support for Inference's custom non-deterministic
//!   intrinsics. This is a fork of LLVM's llc tool.
//! - **rust-lld** - WebAssembly linker from the Rust toolchain, specifically the wasm-ld flavor.
//!
//! These binaries must be available in the `bin/` directory relative to the executable.
//!
//! # Platform Considerations
//!
//! - **Linux**: Requires libLLVM shared library in `lib/` directory. Uses `LD_LIBRARY_PATH`.
//! - **macOS**: Can use system LLVM or bundled libraries. Uses `DYLD_LIBRARY_PATH`.
//! - **Windows**: DLLs must be in `bin/` directory alongside executables. No path configuration needed.
//!
//! # Compilation Pipeline
//!
//! ## Stage 1: IR Emission
//!
//! The LLVM module is serialized to a temporary `.ll` file (LLVM IR text format).
//! The module is configured with the `wasm32-unknown-unknown` target triple.
//!
//! ## Stage 2: Object Compilation (inf-llc)
//!
//! The inf-llc compiler processes the IR file with these arguments:
//! - `-mcpu=mvp` - Target WebAssembly MVP (Minimum Viable Product) feature set
//! - `-filetype=obj` - Output object file format
//! - `-O{0-3}` - Optimization level (0=none, 3=aggressive)
//!
//! Output: `.o` WebAssembly object file
//!
//! ## Stage 3: Linking (rust-lld)
//!
//! The rust-lld linker combines the object file into a final WebAssembly module:
//! - `-flavor wasm` - Use WebAssembly linker mode
//! - `--no-entry` - Reactor model (no implicit `_start` function)
//! - `--export=main` - Explicitly export `main` function if present
//!
//! Output: `.wasm` WebAssembly module
//!
//! ## Stage 4: Cleanup
//!
//! Read the final WASM bytes and remove temporary files. The WASM module is returned
//! as a byte vector.
//!
//! # WebAssembly Execution Model
//!
//! Inference uses the **reactor model** rather than the command model for WebAssembly modules.
//!
//! ## Command Model (Rust, Zig, C with WASI)
//!
//! Languages like Rust and Zig targeting `wasm32-wasi` generate a `_start` entry point:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  WASM Module (wasm32-wasi)              │
//! │                                         │
//! │  _start() ──┬──> runtime init           │
//! │  (entry)    ├──> main()                 │
//! │             └──> exit syscall           │
//! └─────────────────────────────────────────┘
//!
//! Execution: wasmtime module.wasm
//!            (automatically invokes _start)
//! ```
//!
//! ## Reactor Model (Inference)
//!
//! Inference targets `wasm32-unknown-unknown` and produces modules without an entry point.
//! Functions marked `pub` are exported and can be called individually:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │  WASM Module (wasm32-unknown-unknown)   │
//! │                                         │
//! │  Exports:                               │
//! │    ├─ main  (pub fn main)               │
//! │    └─ foo   (pub fn foo)                │
//! │                                         │
//! │  Internal (not exported):               │
//! │    └─ bar   (fn bar)                    │
//! └─────────────────────────────────────────┘
//!
//! Execution: wasmtime --invoke main module.wasm
//!            (caller explicitly invokes exported function)
//! ```
//!
//! Note: The `--invoke` argument requires the function name. Multiple exported
//! functions can exist in the same module, and the caller chooses which to invoke.
//!
//! ## Why Reactor Model?
//!
//! - **Simplicity**: No runtime initialization overhead
//! - **Flexibility**: Multiple entry points, caller chooses which function to invoke
//! - **Embedding**: Better suited for embedding in host applications
//! - **Verification**: Aligns with formal verification where functions are verified individually
//!
//! ## Linker Flags
//!
//! - `--no-entry`: Tells LLD there's no `_start` function (reactor mode)
//! - `--export=main`: Explicitly exports `main` if present (LLD creates argc/argv wrapper)
//!
//! ## Future Consideration
//!
//! If WASI command-style execution is needed, the compiler would need to:
//! 1. Generate a `_start` function calling `main`
//! 2. Remove `--no-entry` flag
//! 3. Optionally switch target to `wasm32-wasi`

use std::{path::PathBuf, process::Command};

use inkwell::{module::Module, targets::TargetTriple};
use tempfile::tempdir;

/// Compiles an LLVM module to WebAssembly bytecode via external toolchain.
///
/// This function orchestrates the complete compilation pipeline from LLVM IR to WASM,
/// handling temporary file management and tool invocation.
///
/// # Compilation Stages
///
/// 1. **IR emission** - Write LLVM module to temporary `.ll` file
/// 2. **Object compilation** - Invoke inf-llc with target wasm32-unknown-unknown
/// 3. **Linking** - Invoke rust-lld with wasm flavor to produce final module
/// 4. **Cleanup** - Read WASM bytes and remove temporary object file
///
/// # Parameters
///
/// - `module` - LLVM module containing the IR to compile
/// - `output_fname` - Base filename for intermediate files (extensions added automatically)
/// - `optimization_level` - LLVM optimization level (0-3, clamped to max 3)
/// - `has_main` - Whether to export a `main` function (only if the module contains one)
///
/// # Returns
///
/// WebAssembly bytecode as a byte vector
///
/// # Errors
///
/// Returns an error if:
/// - Required binaries (inf-llc, rust-lld) are not found
/// - Compilation or linking fails (non-zero exit status)
/// - File I/O operations fail
/// - Temporary directory creation fails
///
/// # Error Details
///
/// When compilation or linking fails, the error message includes:
/// - Exit status of the failed tool
/// - Complete stderr output from the tool
///
/// This information is essential for debugging LLVM IR issues or linker problems.
/// Common failures include:
/// - Invalid LLVM IR syntax
/// - Undefined references in linking
/// - Unsupported WebAssembly features
///
/// # Example Error Messages
///
/// ```text
/// Error: inf-llc failed with status: exit status: 1
/// stderr: <input>:5:10: error: expected value token
///   ret i32 %invalid_var
///           ^
/// ```
///
/// ```text
/// Error: rust-lld failed with status: exit status: 1
/// stderr: wasm-ld: error: undefined symbol: external_function
/// ```
#[allow(clippy::similar_names)]
pub(crate) fn compile_to_wasm(
    module: &Module,
    output_fname: &str,
    optimization_level: u32,
    has_main: bool,
) -> anyhow::Result<Vec<u8>> {
    let llc_path = get_inf_llc_path()?;
    let temp_dir = tempdir()?;
    let obj_path = temp_dir.path().join(output_fname).with_extension("o");
    let ir_path = temp_dir.path().join(output_fname).with_extension("ll");
    let triple = TargetTriple::create("wasm32-unknown-unknown");
    module.set_triple(&triple);
    let ir_str = module.print_to_string().to_string();
    std::fs::write(&ir_path, ir_str)?;
    let opt_flag = format!("-O{}", optimization_level.min(3));
    let mut llc_cmd = Command::new(&llc_path);
    configure_llvm_env(&mut llc_cmd)?;
    let output = llc_cmd
        // .arg("-march=wasm32") // same as triple
        .arg("-mcpu=mvp")
        // .arg("-mattr=+mutable-globals") // https://doc.rust-lang.org/beta/rustc/platform-support/wasm32v1-none.html
        .arg("-filetype=obj")
        .arg(&ir_path)
        .arg(&opt_flag)
        .arg("-o")
        .arg(&obj_path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "inf-llc failed with status: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let rust_lld_path = get_rust_lld_path()?;
    let wasm_path = temp_dir.path().join(output_fname).with_extension("wasm");
    let mut lld_cmd = Command::new(&rust_lld_path);
    configure_llvm_env(&mut lld_cmd)?;
    lld_cmd
        .arg("-flavor")
        .arg("wasm")
        .arg(&obj_path)
        .arg("--no-entry");
    if has_main {
        lld_cmd.arg("--export=main");
    }
    let wasm_lld_output = lld_cmd.arg("-o").arg(&wasm_path).output()?;

    if !wasm_lld_output.status.success() {
        return Err(anyhow::anyhow!(
            "rust-lld failed with status: {}\nstderr: {}",
            wasm_lld_output.status,
            String::from_utf8_lossy(&wasm_lld_output.stderr)
        ));
    }

    let wasm_bytes = std::fs::read(&wasm_path)?;
    std::fs::remove_file(obj_path)?;
    Ok(wasm_bytes)
}

/// Locates the inf-llc binary required for compilation.
///
/// This function searches for the inf-llc executable in platform-specific locations
/// relative to the current executable. It handles both regular builds and test builds
/// (where the executable is in a `deps/` subdirectory).
///
/// # Returns
///
/// Absolute path to the inf-llc executable
///
/// # Errors
///
/// Returns an error if inf-llc is not found in any of the expected locations.
pub(crate) fn get_inf_llc_path() -> anyhow::Result<std::path::PathBuf> {
    get_bin_path(
        "inf-llc",
        "This package requires LLVM with Inference intrinsics support.",
    )
}

/// Locates the rust-lld binary required for linking.
///
/// This function searches for the rust-lld executable in platform-specific locations
/// relative to the current executable. It handles both regular builds and test builds
/// (where the executable is in a `deps/` subdirectory).
///
/// # Returns
///
/// Absolute path to the rust-lld executable
///
/// # Errors
///
/// Returns an error if rust-lld is not found in any of the expected locations.
pub(crate) fn get_rust_lld_path() -> anyhow::Result<PathBuf> {
    get_bin_path(
        "rust-lld",
        "This package requires rust-lld to link WebAssembly modules.",
    )
}

/// Generic binary path resolver with multiple search strategies.
///
/// This function implements a multi-strategy search for external binaries:
/// 1. Check build-time hint from `INF_WASM_CODEGEN_BIN_DIR` environment variable
/// 2. Search in `bin/` directory relative to current executable
/// 3. Search in `../bin/` directory (for test executables in `deps/`)
///
/// The search handles platform-specific executable suffixes (.exe on Windows).
///
/// # Parameters
///
/// - `bin_name` - Name of the binary without extension (e.g., "inf-llc")
/// - `not_found_message` - Error message to display if binary is not found
///
/// # Returns
///
/// Absolute path to the binary
///
/// # Errors
///
/// Returns a detailed error message listing all searched locations if the binary
/// is not found.
fn get_bin_path(bin_name: &str, not_found_message: &str) -> anyhow::Result<PathBuf> {
    let exe_suffix = std::env::consts::EXE_SUFFIX;
    let llc_name = format!("{bin_name}{exe_suffix}");

    // First, try the build-time hint if available
    if let Some(bin_dir) = option_env!("INF_WASM_CODEGEN_BIN_DIR") {
        let candidate = PathBuf::from(bin_dir).join(&llc_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

    // Try multiple possible locations:
    // 1. For regular binaries: <exe_dir>/bin/llc
    // 2. For test binaries in deps/: <exe_dir>/../bin/llc
    let candidates = vec![
        exe_dir.join("bin").join(&llc_name), // target/debug/bin/llc or target/release/bin/llc
        exe_dir.parent().map_or_else(
            || exe_dir.join("bin").join(&llc_name),
            |p| p.join("bin").join(&llc_name), // target/debug/bin/llc when exe is in target/debug/deps/
        ),
    ];

    for llc_path in &candidates {
        if llc_path.exists() {
            return Ok(llc_path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "🚫 {bin_name} binary not found\n\
            \n\
            {not_found_message}\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
        exe_path.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

/// Locates the LLVM shared library directory on Linux.
///
/// On Linux, the LLVM shared libraries (libLLVM.so.*) must be available for the
/// external toolchain binaries (inf-llc, rust-lld) to function. This function
/// searches for the library directory relative to the current executable.
///
/// # Search Strategy
///
/// 1. `<executable-dir>/lib/` - Regular build location
/// 2. `<executable-dir>/../lib/` - Test build location (executable in deps/)
///
/// # Returns
///
/// Absolute path to the directory containing LLVM shared libraries
///
/// # Errors
///
/// Returns an error if no library directory is found in the expected locations.
#[cfg(target_os = "linux")]
fn get_llvm_lib_dir() -> anyhow::Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

    let candidates = vec![
        exe_dir.join("bin"), // target/debug/lib or target/release/lib
        exe_dir.parent().map_or_else(
            || exe_dir.join("lib"),
            |p| p.join("lib"), // target/debug/lib when exe is in target/debug/deps/
        ),
    ];

    for lib_path in &candidates {
        if lib_path.exists() {
            return Ok(lib_path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "🚫 LLVM library directory not found\n\
            \n\
            This package requires LLVM shared libraries.\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
        exe_path.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

/// Configures environment variables for spawned LLVM tools on Linux.
///
/// On Linux, external tools need the `LD_LIBRARY_PATH` environment variable set to
/// locate bundled LLVM shared libraries. This function prepends the library directory
/// to the existing `LD_LIBRARY_PATH` (if any).
///
/// # Parameters
///
/// - `cmd` - Command to configure with appropriate environment variables
///
/// # Errors
///
/// Returns an error if the library directory cannot be located.
#[cfg(target_os = "linux")]
fn configure_llvm_env(cmd: &mut Command) -> anyhow::Result<()> {
    let lib_dir = get_llvm_lib_dir()?;
    let lib_dir_str = lib_dir.to_string_lossy();

    // Prepend to LD_LIBRARY_PATH
    let ld_library_path = if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
        format!("{lib_dir_str}:{existing}")
    } else {
        lib_dir_str.to_string()
    };

    cmd.env("LD_LIBRARY_PATH", ld_library_path);
    Ok(())
}

/// Configures environment variables for spawned LLVM tools on macOS.
///
/// On macOS, this function checks if a custom LLVM installation is specified via
/// the `LLVM_SYS_211_PREFIX` environment variable (typically set for Homebrew LLVM).
/// If found, it configures `DYLD_LIBRARY_PATH` to locate the LLVM libraries.
///
/// # Parameters
///
/// - `cmd` - Command to configure with appropriate environment variables
///
/// # Returns
///
/// Always returns Ok(()) as environment configuration is optional on macOS.
#[cfg(target_os = "macos")]
#[allow(clippy::unnecessary_wraps)]
fn configure_llvm_env(cmd: &mut Command) -> anyhow::Result<()> {
    // On macOS, check if LLVM is installed via Homebrew
    if let Ok(llvm_prefix) = std::env::var("LLVM_SYS_211_PREFIX") {
        let lib_dir = std::path::Path::new(&llvm_prefix).join("lib");
        if lib_dir.exists() {
            let lib_dir_str = lib_dir.to_string_lossy();
            let dyld_library_path = if let Ok(existing) = std::env::var("DYLD_LIBRARY_PATH") {
                format!("{lib_dir_str}:{existing}")
            } else {
                lib_dir_str.to_string()
            };
            cmd.env("DYLD_LIBRARY_PATH", dyld_library_path);
        }
    }
    Ok(())
}

/// Configures environment variables for spawned LLVM tools on Windows.
///
/// On Windows, DLL loading uses the executable's directory by default, so all required
/// DLLs should be placed in the `bin/` directory alongside the executables. No
/// environment variable configuration is needed.
///
/// # Parameters
///
/// - `_cmd` - Command to configure (unused on Windows)
///
/// # Returns
///
/// Always returns Ok(()) as no configuration is needed on Windows.
#[cfg(target_os = "windows")]
#[allow(clippy::unnecessary_wraps)]
fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    // On Windows, DLLs are placed in the same directory as the executables,
    // so Windows will find them automatically. No environment modification needed.
    Ok(())
}

/// Fallback environment configuration for unsupported platforms.
///
/// This is a no-op implementation for platforms other than Linux, macOS, and Windows.
/// Compilation may or may not work on these platforms depending on system configuration.
///
/// # Parameters
///
/// - `_cmd` - Command to configure (unused)
///
/// # Returns
///
/// Always returns Ok(()) as no configuration is attempted.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    Ok(())
}
