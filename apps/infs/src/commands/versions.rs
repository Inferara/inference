//! Versions command for the infs CLI.
//!
//! Lists available toolchain versions from the release manifest with
//! stability markers and platform availability information.
//!
//! ## Usage
//!
//! ```bash
//! infs versions           # List all available versions
//! infs versions --stable  # List only stable versions
//! infs versions --json    # Output in JSON format
//! ```
//!
//! ## Output Format
//!
//! ```text
//! Available toolchain versions:
//!
//!   0.2.0 (stable) [linux, macos, windows] *
//!   0.1.0 (stable) [linux] *
//!   0.3.0-alpha (prerelease) [linux]
//!
//!   * = available for current platform (linux)
//! ```

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::toolchain::Platform;
use crate::toolchain::manifest::{fetch_manifest, sorted_versions};

/// Arguments for the versions command.
#[derive(Args)]
pub struct VersionsArgs {
    /// Show only stable versions.
    #[clap(long, short = 's')]
    pub stable: bool,

    /// Show versions in JSON format.
    #[clap(long, short = 'j')]
    pub json: bool,
}

/// Version information for JSON output.
#[derive(Debug, Clone, Serialize)]
struct VersionInfo {
    version: String,
    stable: bool,
    platforms: Vec<String>,
    available_for_current: bool,
}

/// Executes the versions command.
///
/// # Process
///
/// 1. Detect the current platform
/// 2. Fetch the release manifest from the distribution server
/// 3. Sort versions by semver (newest first)
/// 4. Filter by stability if --stable flag is set
/// 5. Output in text or JSON format
///
/// # Errors
///
/// Returns an error if:
/// - Platform detection fails
/// - Manifest fetch fails (network error, parsing error, etc.)
pub async fn execute(args: &VersionsArgs) -> Result<()> {
    let platform = Platform::detect()?;
    let manifest = fetch_manifest().await?;

    if args.json {
        output_json(&manifest, args.stable, platform)?;
    } else {
        output_text(&manifest, args.stable, platform);
    }

    Ok(())
}

/// Outputs version information in JSON format.
fn output_json(
    manifest: &crate::toolchain::manifest::Manifest,
    stable_only: bool,
    platform: Platform,
) -> Result<()> {
    let versions = sorted_versions(manifest);

    let version_infos: Vec<VersionInfo> = versions
        .iter()
        .filter(|v| !stable_only || v.stable)
        .map(|v| VersionInfo {
            version: v.version.clone(),
            stable: v.stable,
            platforms: v
                .available_platforms()
                .into_iter()
                .map(String::from)
                .collect(),
            available_for_current: v.has_platform(platform),
        })
        .collect();

    let json = serde_json::to_string_pretty(&version_infos)?;
    println!("{json}");
    Ok(())
}

/// Outputs version information in text format.
fn output_text(
    manifest: &crate::toolchain::manifest::Manifest,
    stable_only: bool,
    platform: Platform,
) {
    let versions = sorted_versions(manifest);

    let filtered: Vec<_> = versions
        .iter()
        .filter(|v| !stable_only || v.stable)
        .collect();

    if filtered.is_empty() {
        if stable_only {
            println!("No stable versions available.");
        } else {
            println!("No versions available.");
        }
        return;
    }

    println!("Available toolchain versions:");
    println!();

    let os = platform.os();
    let mut has_current_platform = false;

    for version in &filtered {
        let stability = if version.stable {
            "(stable)"
        } else {
            "(prerelease)"
        };

        let platforms = version.available_platforms();
        let platform_list = if platforms.is_empty() {
            String::new()
        } else {
            format!("[{}]", platforms.join(", "))
        };

        let available_marker = if version.has_platform(platform) {
            has_current_platform = true;
            " *"
        } else {
            ""
        };

        println!(
            "  {} {} {}{}",
            version.version, stability, platform_list, available_marker
        );
    }

    if has_current_platform {
        println!();
        println!("  * = available for current platform ({os})");
    }
}
