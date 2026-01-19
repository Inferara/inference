#![warn(clippy::pedantic)]

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

use anyhow::{Context, Result};
use clap::Args;
use std::path::Path;

use crate::toolchain::{
    Platform, ToolchainPaths, download_file, fetch_artifact, verify_checksum,
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

    if paths.is_version_installed(&version) {
        println!("Toolchain version {version} is already installed.");
        return Ok(());
    }

    println!("Installing toolchain version {version} for {platform}...");

    let archive_filename = format!("toolchain-{version}-{platform}.zip");
    let archive_path = paths.download_path(&archive_filename);

    println!("Downloading from {}...", artifact.url);
    download_file(&artifact.url, &archive_path, artifact.size).await?;

    println!("Verifying checksum...");
    verify_checksum(&archive_path, &artifact.sha256)?;

    println!("Extracting...");
    let toolchain_dir = paths.toolchain_dir(&version);
    extract_zip(&archive_path, &toolchain_dir)?;

    set_executable_permissions(&toolchain_dir)?;

    let installed_versions = paths.list_installed_versions()?;
    let is_first_install = installed_versions.len() == 1 && installed_versions[0] == version;
    let current_default = paths.get_default_version()?;

    if is_first_install || current_default.is_none() {
        println!("Setting {version} as default toolchain...");
        paths.set_default_version(&version)?;
        update_symlinks(&paths, &version)?;
    }

    println!("Toolchain {version} installed successfully.");

    if current_default.is_some() && current_default.as_deref() != Some(&version) {
        println!(
            "Run 'infs default {version}' to make it the default toolchain."
        );
    }

    std::fs::remove_file(&archive_path).ok();

    Ok(())
}

/// Extracts a ZIP archive to the destination directory.
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("Failed to open archive: {}", archive_path.display()))?;

    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive: {}", archive_path.display()))?;

    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read archive entry {i}"))?;

        let entry_path = entry
            .enclosed_name()
            .with_context(|| format!("Invalid entry path in archive: entry {i}"))?;

        let output_path = dest_dir.join(entry_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&output_path)
                .with_context(|| format!("Failed to create directory: {}", output_path.display()))?;
        } else {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            let mut outfile = std::fs::File::create(&output_path)
                .with_context(|| format!("Failed to create file: {}", output_path.display()))?;

            std::io::copy(&mut entry, &mut outfile)
                .with_context(|| format!("Failed to extract: {}", output_path.display()))?;
        }
    }

    Ok(())
}

/// Sets executable permissions on binary files (Unix only).
#[cfg(unix)]
fn set_executable_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = dir.join("bin");
    if !bin_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&bin_dir)
        .with_context(|| format!("Failed to read bin directory: {}", bin_dir.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read directory entry")?;
        let path = entry.path();
        if path.is_file() {
            let mut perms = std::fs::metadata(&path)
                .with_context(|| format!("Failed to get metadata: {}", path.display()))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms)
                .with_context(|| format!("Failed to set permissions: {}", path.display()))?;
        }
    }

    Ok(())
}

/// Sets executable permissions (no-op on Windows).
#[cfg(windows)]
fn set_executable_permissions(_dir: &Path) -> Result<()> {
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

    std::fs::create_dir_all(&paths.bin)
        .with_context(|| format!("Failed to create bin directory: {}", paths.bin.display()))?;

    for binary in &binaries {
        paths.create_symlink(version, binary)?;
    }

    Ok(())
}
