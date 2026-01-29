//! Install command for the infs CLI.
//!
//! Downloads and installs a specific version of the Inference toolchain.
//! If no version is specified, installs the latest stable version.
//!
//! ## Usage
//!
//! ```bash
//! infs install          # Install latest stable version
//! infs install 0.1.0    # Install specific version
//! infs install latest   # Explicitly install latest stable
//! ```

use anyhow::Result;
use clap::Args;

use crate::toolchain::conflict::{detect_path_conflicts, format_conflict_warning};
use crate::toolchain::paths::ToolchainMetadata;
use crate::toolchain::{
    Platform, ToolchainPaths, download_file, extract_archive, fetch_artifact,
    set_executable_permissions, verify_checksum,
};

/// Arguments for the install command.
#[derive(Args)]
pub struct InstallArgs {
    /// Version to install (e.g., "0.1.0" or "latest").
    ///
    /// If omitted, installs the latest stable version.
    #[clap(default_value = "latest")]
    pub version: String,
}

/// Executes the install command.
///
/// # Process
///
/// 1. Detect the current platform
/// 2. Fetch the release manifest
/// 3. Find the artifact for the requested version and platform
/// 4. Download the archive with progress display
/// 5. Verify the SHA256 checksum
/// 6. Extract to the toolchains directory
/// 7. Set as default if it's the first installation
///
/// # Errors
///
/// Returns an error if:
/// - Platform detection fails
/// - Manifest fetch fails
/// - Version is not found
/// - Download fails
/// - Checksum verification fails
/// - Extraction fails
pub async fn execute(args: &InstallArgs) -> Result<()> {
    let platform = Platform::detect()?;
    let paths = ToolchainPaths::new()?;

    paths.ensure_directories()?;

    let version_arg = if args.version == "latest" {
        None
    } else {
        Some(args.version.as_str())
    };

    println!("Fetching release manifest...");
    let (version, artifact) = fetch_artifact(version_arg, platform).await?;

    // Handle the case when the requested version is already installed.
    // If no default toolchain is set (e.g., user manually removed the default file
    // or installed via another method), we set this version as default to ensure
    // the toolchain is usable. This provides a graceful recovery path.
    if paths.is_version_installed(&version) {
        let current_default = paths.get_default_version()?;
        if current_default.is_none() {
            println!("Toolchain version {version} is already installed.");
            println!("Setting {version} as default toolchain...");
            paths.set_default_version(&version)?;
            paths.update_symlinks(&version)?;
        } else {
            println!("Toolchain version {version} is already installed.");
        }
        return Ok(());
    }

    println!("Installing toolchain version {version} for {platform}...");

    let archive_filename = artifact.filename();
    let archive_path = paths.download_path(archive_filename);

    println!("Downloading from {}...", artifact.url);
    download_file(&artifact.url, &archive_path).await?;

    println!("Verifying checksum...");
    verify_checksum(&archive_path, &artifact.sha256)?;

    println!("Extracting...");
    let toolchain_dir = paths.toolchain_dir(&version);
    extract_archive(&archive_path, &toolchain_dir)?;

    set_executable_permissions(&toolchain_dir)?;

    let metadata = ToolchainMetadata::now();
    paths.write_metadata(&version, &metadata)?;

    let installed_versions = paths.list_installed_versions()?;
    let is_first_install = installed_versions.len() == 1 && installed_versions[0] == version;
    let current_default = paths.get_default_version()?;

    if is_first_install || current_default.is_none() {
        println!("Setting {version} as default toolchain...");
        paths.set_default_version(&version)?;
        paths.update_symlinks(&version)?;
    }

    println!("Toolchain {version} installed successfully.");

    if is_first_install {
        println!();
        configure_shell_path(&paths);
    }

    let conflicts = detect_path_conflicts(&paths.bin);
    if !conflicts.is_empty() {
        eprintln!();
        eprintln!("{}", format_conflict_warning(&conflicts));
    }

    if current_default.is_some() && current_default.as_deref() != Some(&version) {
        println!("Run 'infs default {version}' to make it the default toolchain.");
    }

    std::fs::remove_file(&archive_path).ok();

    Ok(())
}

/// Configures the user's PATH environment.
///
/// On Unix systems, attempts to automatically add the bin directory to PATH
/// by modifying the user's shell profile. On Windows, modifies the user's
/// PATH environment variable in the registry.
fn configure_shell_path(paths: &ToolchainPaths) {
    use crate::toolchain::shell::{configure_path, format_result_message};

    match configure_path(&paths.bin) {
        Ok(result) => {
            let message = format_result_message(&result, &paths.bin);
            println!("{message}");
        }
        Err(e) => {
            eprintln!("Warning: Could not configure PATH automatically: {e}");
            #[cfg(unix)]
            {
                println!("To use the toolchain, add to your shell profile:");
                println!("  export PATH=\"{}:$PATH\"", paths.bin.display());
            }
            #[cfg(windows)]
            {
                println!("To use the toolchain, add to your PATH environment variable:");
                println!("  {}", paths.bin.display());
            }
        }
    }
}
