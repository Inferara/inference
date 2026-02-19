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
//!   0.1.0    (installed today)
//! * 0.2.0    (default, installed yesterday)
//! ```

use anyhow::Result;

use crate::toolchain::ToolchainPaths;

/// Executes the list command.
///
/// Lists all installed toolchain versions and marks the default with an asterisk.
/// Also displays the installation date for each version if available.
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
        let metadata = paths.read_metadata(version);

        let mut info_parts = Vec::new();
        if is_default {
            info_parts.push("default".to_string());
        }
        if let Some(meta) = metadata {
            info_parts.push(format!("installed {}", meta.installed_ago()));
        }

        let marker = if is_default { "*" } else { " " };
        if info_parts.is_empty() {
            println!("{marker} {version}");
        } else {
            println!("{marker} {version}    ({})", info_parts.join(", "));
        }
    }

    if default_version.is_none() {
        println!();
        println!("No default toolchain set. Run 'infs default <version>' to set one.");
    }

    Ok(())
}
