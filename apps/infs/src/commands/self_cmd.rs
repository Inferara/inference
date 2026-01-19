#![warn(clippy::pedantic)]

//! Self command for the infs CLI.
//!
//! Provides subcommands for managing the `infs` binary itself.
//!
//! ## Usage
//!
//! ```bash
//! infs self update    # Update infs to the latest version
//! ```

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::toolchain::{Platform, ToolchainPaths, download_file, fetch_manifest, verify_checksum};

/// Arguments for the self command.
#[derive(Args)]
pub struct SelfArgs {
    #[command(subcommand)]
    pub command: SelfCommand,
}

/// Subcommands for self management.
#[derive(Subcommand)]
pub enum SelfCommand {
    /// Update infs to the latest version.
    Update,
}

/// Executes the self command.
///
/// # Errors
///
/// Returns an error if the subcommand fails.
pub async fn execute(args: &SelfArgs) -> Result<()> {
    match &args.command {
        SelfCommand::Update => execute_update().await,
    }
}

/// Executes the self update subcommand.
///
/// # Process
///
/// 1. Fetch the release manifest
/// 2. Compare current version with latest
/// 3. If newer version available, download it
/// 4. Verify checksum
/// 5. Replace current binary
///
/// ## Windows Strategy
///
/// On Windows, the running binary cannot be replaced directly.
/// Instead, we rename the current binary to `infs.old` and place
/// the new binary in its place. The old binary can be deleted
/// on the next run or manually.
///
/// # Errors
///
/// Returns an error if:
/// - Manifest fetch fails
/// - No infs artifact for current platform
/// - Download fails
/// - Checksum verification fails
/// - Binary replacement fails
async fn execute_update() -> Result<()> {
    let platform = Platform::detect()?;
    let paths = ToolchainPaths::new()?;
    paths.ensure_directories()?;

    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current infs version: {current_version}");

    println!("Checking for updates...");
    let manifest = fetch_manifest().await?;

    let latest_version = match &manifest.latest_infs {
        Some(v) => v.clone(),
        None => manifest.latest_stable.clone(),
    };

    if latest_version == current_version {
        println!("infs is already up to date.");
        return Ok(());
    }

    let current_semver = semver::Version::parse(current_version)
        .with_context(|| format!("Invalid current version: {current_version}"))?;
    let latest_semver = semver::Version::parse(&latest_version)
        .with_context(|| format!("Invalid latest version: {latest_version}"))?;

    if current_semver >= latest_semver {
        println!("infs is already up to date (current: {current_version}, available: {latest_version}).");
        return Ok(());
    }

    let artifact = manifest
        .find_infs_artifact(platform)
        .with_context(|| format!("No infs binary available for platform {platform}"))?;

    println!("Updating infs from {current_version} to {latest_version}...");

    let download_filename = format!("infs-{latest_version}-{platform}.zip");
    let download_path = paths.download_path(&download_filename);

    println!("Downloading from {}...", artifact.url);
    download_file(&artifact.url, &download_path, artifact.size).await?;

    println!("Verifying checksum...");
    verify_checksum(&download_path, &artifact.sha256)?;

    println!("Extracting...");
    let temp_dir = paths.downloads.join(format!("infs-{latest_version}-temp"));
    extract_zip(&download_path, &temp_dir)?;

    let new_binary_name = format!("infs{}", platform.executable_extension());
    let new_binary_path = temp_dir.join(&new_binary_name);

    if new_binary_path.exists() {
        replace_binary(&new_binary_path, platform)?;
    } else {
        let bin_path = temp_dir.join("bin").join(&new_binary_name);
        if bin_path.exists() {
            replace_binary(&bin_path, platform)?;
        } else {
            bail!(
                "infs binary not found in downloaded archive. Expected at {} or {}",
                new_binary_path.display(),
                bin_path.display()
            );
        }
    }

    std::fs::remove_file(&download_path).ok();
    std::fs::remove_dir_all(&temp_dir).ok();

    println!("Successfully updated infs to {latest_version}.");
    if platform.is_windows() {
        println!("Note: Please restart your terminal to use the new version.");
    }

    Ok(())
}

/// Extracts a ZIP archive to the destination directory.
fn extract_zip(archive_path: &std::path::Path, dest_dir: &std::path::Path) -> Result<()> {
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

/// Replaces the current binary with a new one.
fn replace_binary(new_binary: &std::path::Path, _platform: Platform) -> Result<()> {
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(new_binary)
            .with_context(|| format!("Failed to get metadata: {}", new_binary.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(new_binary, perms)
            .with_context(|| format!("Failed to set permissions: {}", new_binary.display()))?;

        std::fs::rename(new_binary, &current_exe)
            .with_context(|| format!(
                "Failed to replace binary. You may need to run with elevated privileges.\n\
                 Source: {}\n\
                 Destination: {}",
                new_binary.display(),
                current_exe.display()
            ))?;
    }

    #[cfg(windows)]
    {
        let old_binary = current_exe.with_extension("old.exe");

        if old_binary.exists() {
            std::fs::remove_file(&old_binary).ok();
        }

        std::fs::rename(&current_exe, &old_binary)
            .with_context(|| format!(
                "Failed to rename current binary to {}",
                old_binary.display()
            ))?;

        if let Err(e) = std::fs::rename(new_binary, &current_exe) {
            std::fs::rename(&old_binary, &current_exe).ok();
            return Err(e).with_context(|| format!(
                "Failed to install new binary at {}",
                current_exe.display()
            ));
        }
    }

    Ok(())
}
