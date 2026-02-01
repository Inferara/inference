//! Platform-specific environment configuration for external LLVM tools.
//!
//! External toolchain binaries (`inf-llc`, `rust-lld`) may need environment variables
//! set to locate shared libraries. This module configures the appropriate variables
//! per platform before spawning tool processes.
//!
//! # Platform Behavior
//!
//! - **Linux**: Sets `LD_LIBRARY_PATH` to include the bundled `lib/` directory
//! - **macOS**: Sets `DYLD_LIBRARY_PATH` if a Homebrew LLVM prefix is detected
//! - **Windows**: No configuration needed (DLLs co-located with executables)

use std::process::Command;

/// Configures environment variables for spawned LLVM tools on Linux.
///
/// On Linux, `inf-llc` and `rust-lld` need the `LD_LIBRARY_PATH` set to locate
/// bundled LLVM shared libraries (`libLLVM.so.*`). This function prepends the
/// library directory to the existing `LD_LIBRARY_PATH`.
///
/// # Errors
///
/// Returns an error if the library directory cannot be located.
#[cfg(target_os = "linux")]
pub(crate) fn configure_llvm_env(cmd: &mut Command) -> anyhow::Result<()> {
    let lib_dir = get_llvm_lib_dir()?;
    let lib_dir_str = lib_dir.to_string_lossy();

    let ld_library_path = if let Ok(existing) = std::env::var("LD_LIBRARY_PATH") {
        format!("{lib_dir_str}:{existing}")
    } else {
        lib_dir_str.to_string()
    };

    cmd.env("LD_LIBRARY_PATH", ld_library_path);
    Ok(())
}

/// Locates the LLVM shared library directory on Linux.
///
/// Searches for the library directory in:
/// 1. `<exe_dir>/lib/` — regular build layout
/// 2. `<exe_dir>/../lib/` — test builds (executable in `deps/`)
///
/// # Errors
///
/// Returns an error if no library directory is found.
#[cfg(target_os = "linux")]
fn get_llvm_lib_dir() -> anyhow::Result<std::path::PathBuf> {
    let dir = super::paths::exe_dir()?;

    let candidates = [
        dir.join("lib"),
        dir.parent().map_or_else(
            || dir.join("lib"),
            |p| p.join("lib"),
        ),
    ];

    for lib_path in &candidates {
        if lib_path.exists() {
            return Ok(lib_path.clone());
        }
    }

    Err(anyhow::anyhow!(
        "LLVM library directory not found\n\
            \n\
            This package requires LLVM shared libraries.\n\n\
            Executable directory: {}\n\
            Searched locations:\n  - {}\n  - {}",
        dir.display(),
        candidates[0].display(),
        candidates[1].display()
    ))
}

/// Configures environment variables for spawned LLVM tools on macOS.
///
/// Checks for a custom LLVM installation via the `LLVM_SYS_211_PREFIX` environment
/// variable (typically set for Homebrew LLVM). If found, configures `DYLD_LIBRARY_PATH`.
///
/// # Errors
///
/// Always returns `Ok(())` as environment configuration is optional on macOS.
#[cfg(target_os = "macos")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn configure_llvm_env(cmd: &mut Command) -> anyhow::Result<()> {
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
/// On Windows, DLL loading uses the executable's directory by default. All required
/// DLLs should be placed in the `bin/` directory alongside the executables.
///
/// # Errors
///
/// Always returns `Ok(())` as no configuration is needed on Windows.
#[cfg(target_os = "windows")]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    Ok(())
}

/// Fallback environment configuration for unsupported platforms.
///
/// No-op implementation for platforms other than Linux, macOS, and Windows.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn configure_llvm_env(_cmd: &mut Command) -> anyhow::Result<()> {
    Ok(())
}
