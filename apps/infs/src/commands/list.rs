#![warn(clippy::pedantic)]

//! List command for the infs CLI.
//!
//! Displays installed toolchain versions and indicates the current default.
//!
//! ## Usage
//!
//! ```bash
//! infs list
//! ```
//!
//! ## Output Format
//!
//! ```text
//! Installed toolchains:
//!   0.1.0
//! * 0.2.0 (default)
//! ```

use anyhow::Result;

use crate::toolchain::ToolchainPaths;

/// Executes the list command.
///
/// Lists all installed toolchain versions and marks the default with an asterisk.
///
/// # Errors
///
/// Returns an error if the toolchains directory cannot be read.
#[allow(clippy::unnecessary_wraps, clippy::unused_async)]
pub async fn execute() -> Result<()> {
    let paths = ToolchainPaths::new()?;
    let versions = paths.list_installed_versions()?;
    let default_version = paths.get_default_version()?;

    if versions.is_empty() {
        println!("No toolchains installed.");
        println!();
        println!("Run 'infs install' to install the latest toolchain.");
        return Ok(());
    }

    println!("Installed toolchains:");
    println!();

    for version in &versions {
        let is_default = default_version.as_deref() == Some(version.as_str());
        if is_default {
            println!("* {version} (default)");
        } else {
            println!("  {version}");
        }
    }

    if default_version.is_none() {
        println!();
        println!("No default toolchain set. Run 'infs default <version>' to set one.");
    }

    Ok(())
}
