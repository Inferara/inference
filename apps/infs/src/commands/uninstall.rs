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

use crate::toolchain::ToolchainPaths;

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
    std::fs::remove_dir_all(&toolchain_dir).with_context(|| {
        format!(
            "Failed to remove toolchain directory: {}",
            toolchain_dir.display()
        )
    })?;

    // Handle symlinks and default version
    if is_default {
        let remaining_versions = paths.list_installed_versions()?;

        if remaining_versions.is_empty() {
            std::fs::remove_file(paths.default_file()).ok();
            paths.remove_symlinks()?;
            println!("No toolchains remaining. Default has been cleared.");
        } else {
            let new_default = remaining_versions
                .last()
                .expect("remaining_versions is non-empty");
            paths.set_default_version(new_default)?;
            paths.update_symlinks(new_default)?;
            println!("Default toolchain changed to {new_default}.");
        }
    } else {
        // Even if not uninstalling the default, validate and repair symlinks.
        // This handles edge cases where symlinks might be broken.
        let broken_symlinks = paths.validate_symlinks();
        if !broken_symlinks.is_empty() {
            println!("Repairing broken symlinks...");
            paths.repair_symlinks()?;
        }
    }

    println!("Toolchain {version} uninstalled successfully.");

    Ok(())
}
