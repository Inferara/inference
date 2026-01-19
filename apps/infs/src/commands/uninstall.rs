#![warn(clippy::pedantic)]

//! Uninstall command for the infs CLI.
//!
//! Removes an installed toolchain version from the system.
//!
//! ## Usage
//!
//! ```bash
//! infs uninstall 0.1.0    # Remove version 0.1.0
//! ```

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::toolchain::{Platform, ToolchainPaths};

/// Arguments for the uninstall command.
#[derive(Args)]
pub struct UninstallArgs {
    /// Version to uninstall (e.g., "0.1.0").
    pub version: String,
}

/// Executes the uninstall command.
///
/// # Process
///
/// 1. Check if the version is installed
/// 2. Check if it's the current default version
/// 3. Remove the toolchain directory
/// 4. Update symlinks if necessary
///
/// # Errors
///
/// Returns an error if:
/// - The version is not installed
/// - Directory removal fails
#[allow(clippy::unused_async)]
pub async fn execute(args: &UninstallArgs) -> Result<()> {
    let paths = ToolchainPaths::new()?;
    let version = &args.version;

    if !paths.is_version_installed(version) {
        bail!("Toolchain version {version} is not installed.");
    }

    let default_version = paths.get_default_version()?;
    let is_default = default_version.as_deref() == Some(version);

    if is_default {
        println!("Warning: {version} is the current default toolchain.");
    }

    println!("Uninstalling toolchain version {version}...");

    let toolchain_dir = paths.toolchain_dir(version);
    std::fs::remove_dir_all(&toolchain_dir)
        .with_context(|| format!("Failed to remove toolchain directory: {}", toolchain_dir.display()))?;

    if is_default {
        let remaining_versions = paths.list_installed_versions()?;

        if remaining_versions.is_empty() {
            std::fs::remove_file(paths.default_file()).ok();
            remove_symlinks(&paths)?;
            println!("No toolchains remaining. Default has been cleared.");
        } else {
            let new_default = remaining_versions.last().unwrap_or(&remaining_versions[0]);
            paths.set_default_version(new_default)?;
            update_symlinks(&paths, new_default)?;
            println!("Default toolchain changed to {new_default}.");
        }
    }

    println!("Toolchain {version} uninstalled successfully.");

    Ok(())
}

/// Removes all symlinks from the bin directory.
fn remove_symlinks(paths: &ToolchainPaths) -> Result<()> {
    let platform = Platform::detect()?;
    let ext = platform.executable_extension();

    let binaries = [
        format!("inf-llc{ext}"),
        format!("rust-lld{ext}"),
    ];

    for binary in &binaries {
        paths.remove_symlink(binary)?;
    }

    Ok(())
}

/// Updates symlinks in the bin directory to point to the specified version.
fn update_symlinks(paths: &ToolchainPaths, version: &str) -> Result<()> {
    let platform = Platform::detect()?;
    let ext = platform.executable_extension();

    let binaries = [
        format!("inf-llc{ext}"),
        format!("rust-lld{ext}"),
    ];

    for binary in &binaries {
        paths.create_symlink(version, binary)?;
    }

    Ok(())
}
