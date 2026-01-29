//! Default command for the infs CLI.
//!
//! Sets the default toolchain version to use for compilation.
//!
//! ## Usage
//!
//! ```bash
//! infs default 0.2.0    # Set version 0.2.0 as default
//! ```

use anyhow::{Result, bail};
use clap::Args;

use crate::toolchain::ToolchainPaths;
use crate::toolchain::manifest::{fetch_manifest, find_version};

/// Arguments for the default command.
#[derive(Args)]
pub struct DefaultArgs {
    /// Version to set as default (e.g., "0.2.0").
    pub version: String,
}

/// Executes the default command.
///
/// # Process
///
/// 1. Verify the version is installed
/// 2. Update the default file
/// 3. Update symlinks in the bin directory
///
/// # Errors
///
/// Returns an error if:
/// - The version is not installed
/// - The version does not exist in the release manifest
/// - Symlink creation fails
pub async fn execute(args: &DefaultArgs) -> Result<()> {
    let paths = ToolchainPaths::new()?;
    let version = &args.version;

    if !paths.is_version_installed(version) {
        match fetch_manifest().await {
            Ok(manifest) => {
                if find_version(&manifest, version).is_some() {
                    // Version exists in manifest but not installed locally
                    bail!(
                        "Toolchain version {version} is not installed.\n\
                         Run 'infs install {version}' to install it first."
                    );
                }
                // Version does not exist in manifest at all
                bail!(
                    "Toolchain version {version} does not exist.\n\
                     Run 'infs versions' to see available versions."
                );
            }
            Err(_) => {
                // Network error - graceful degradation with both suggestions
                bail!(
                    "Toolchain version {version} is not installed.\n\
                     Run 'infs install {version}' to install it, or 'infs versions' to see available versions."
                );
            }
        }
    }

    let current_default = paths.get_default_version()?;
    if current_default.as_deref() == Some(version.as_str()) {
        println!("Toolchain {version} is already the default.");
        return Ok(());
    }

    paths.set_default_version(version)?;
    paths.update_symlinks(version)?;

    println!("Default toolchain set to {version}.");

    Ok(())
}
