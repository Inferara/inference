#![warn(clippy::pedantic)]

//! Path management for the infs toolchain.
//!
//! This module provides utilities for managing toolchain installation paths.
//! The default root directory is `~/.infs/`, which can be overridden by
//! setting the `INFS_HOME` environment variable.
//!
//! ## Directory Structure
//!
//! ```text
//! ~/.infs/                    # Root directory (or INFS_HOME)
//!   toolchains/               # Installed toolchain versions
//!     0.1.0/                  # Version-specific installation
//!       bin/
//!         inf-llc
//!         rust-lld
//!     0.2.0/
//!       ...
//!   bin/                      # Symlinks to default toolchain binaries
//!   downloads/                # Download cache
//!   default                   # File containing default version string
//! ```

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Environment variable to override the default toolchain root directory.
pub const INFS_HOME_ENV: &str = "INFS_HOME";

/// Manages paths for toolchain installations.
///
/// This struct provides access to all toolchain-related directories and files,
/// ensuring consistent path construction across the codebase.
#[derive(Debug, Clone)]
pub struct ToolchainPaths {
    /// Root directory for all toolchain data (`~/.infs` or `INFS_HOME`).
    pub root: PathBuf,
    /// Directory containing installed toolchain versions.
    pub toolchains: PathBuf,
    /// Directory for binary symlinks to the default toolchain.
    pub bin: PathBuf,
    /// Directory for cached downloads.
    pub downloads: PathBuf,
}

impl ToolchainPaths {
    /// Creates a new `ToolchainPaths` instance.
    ///
    /// The root directory is determined by:
    /// 1. The `INFS_HOME` environment variable if set
    /// 2. Otherwise, `~/.infs` in the user's home directory
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn new() -> Result<Self> {
        let root = if let Ok(home) = std::env::var(INFS_HOME_ENV) {
            PathBuf::from(home)
        } else {
            dirs::home_dir()
                .context("Cannot determine home directory. Set INFS_HOME environment variable.")?
                .join(".infs")
        };

        Ok(Self::with_root(root))
    }

    /// Creates a new `ToolchainPaths` instance with a specific root directory.
    ///
    /// This is useful for testing or when the root directory is known in advance.
    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            toolchains: root.join("toolchains"),
            bin: root.join("bin"),
            downloads: root.join("downloads"),
            root,
        }
    }

    /// Returns the path to a specific toolchain version's installation directory.
    #[must_use]
    pub fn toolchain_dir(&self, version: &str) -> PathBuf {
        self.toolchains.join(version)
    }

    /// Returns the path to the bin directory within a specific toolchain version.
    #[must_use]
    pub fn toolchain_bin_dir(&self, version: &str) -> PathBuf {
        self.toolchain_dir(version).join("bin")
    }

    /// Returns the path to the file storing the default toolchain version.
    #[must_use]
    pub fn default_file(&self) -> PathBuf {
        self.root.join("default")
    }

    /// Returns the path for a downloaded archive file.
    #[must_use]
    pub fn download_path(&self, filename: &str) -> PathBuf {
        self.downloads.join(filename)
    }

    /// Returns the path to a temporary download file.
    #[must_use]
    #[allow(dead_code)]
    pub fn download_temp_path(&self, filename: &str) -> PathBuf {
        self.downloads.join(format!("{filename}.tmp"))
    }

    /// Checks if a specific toolchain version is installed.
    #[must_use]
    pub fn is_version_installed(&self, version: &str) -> bool {
        self.toolchain_dir(version).exists()
    }

    /// Returns the currently set default toolchain version.
    ///
    /// # Errors
    ///
    /// Returns an error if the default file cannot be read.
    pub fn get_default_version(&self) -> Result<Option<String>> {
        let default_file = self.default_file();
        if !default_file.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&default_file)
            .with_context(|| format!("Failed to read default version from {}", default_file.display()))?;
        let version = content.trim();
        if version.is_empty() {
            Ok(None)
        } else {
            Ok(Some(version.to_string()))
        }
    }

    /// Sets the default toolchain version.
    ///
    /// # Errors
    ///
    /// Returns an error if the default file cannot be written.
    pub fn set_default_version(&self, version: &str) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("Failed to create directory: {}", self.root.display()))?;
        std::fs::write(self.default_file(), version)
            .with_context(|| format!("Failed to write default version to {}", self.default_file().display()))?;
        Ok(())
    }

    /// Lists all installed toolchain versions.
    ///
    /// Returns a sorted list of version strings for all installed toolchains.
    ///
    /// # Errors
    ///
    /// Returns an error if the toolchains directory cannot be read.
    pub fn list_installed_versions(&self) -> Result<Vec<String>> {
        if !self.toolchains.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        let entries = std::fs::read_dir(&self.toolchains)
            .with_context(|| format!("Failed to read toolchains directory: {}", self.toolchains.display()))?;

        for entry in entries {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name()
                && let Some(name_str) = name.to_str()
            {
                versions.push(name_str.to_string());
            }
        }

        versions.sort();
        Ok(versions)
    }

    /// Ensures all required directories exist.
    ///
    /// Creates the root, toolchains, bin, and downloads directories if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if any directory cannot be created.
    pub fn ensure_directories(&self) -> Result<()> {
        for dir in [&self.root, &self.toolchains, &self.bin, &self.downloads] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
        }
        Ok(())
    }

    /// Returns the path to a specific binary within a toolchain version.
    #[must_use]
    pub fn binary_path(&self, version: &str, binary_name: &str) -> PathBuf {
        self.toolchain_bin_dir(version).join(binary_name)
    }

    /// Returns the path to a symlinked binary in the global bin directory.
    #[must_use]
    pub fn symlink_path(&self, binary_name: &str) -> PathBuf {
        self.bin.join(binary_name)
    }

    /// Creates a symlink from the global bin directory to a toolchain binary.
    ///
    /// On Windows, this creates a hard link or copies the file if symlinks are not supported.
    ///
    /// # Errors
    ///
    /// Returns an error if the symlink cannot be created.
    pub fn create_symlink(&self, version: &str, binary_name: &str) -> Result<()> {
        let source = self.binary_path(version, binary_name);
        let target = self.symlink_path(binary_name);

        if !source.exists() {
            return Ok(());
        }

        if target.exists() {
            std::fs::remove_file(&target)
                .with_context(|| format!("Failed to remove existing symlink: {}", target.display()))?;
        }

        create_link(&source, &target)
    }

    /// Removes a symlink from the global bin directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the symlink cannot be removed.
    pub fn remove_symlink(&self, binary_name: &str) -> Result<()> {
        let target = self.symlink_path(binary_name);
        if target.exists() {
            std::fs::remove_file(&target)
                .with_context(|| format!("Failed to remove symlink: {}", target.display()))?;
        }
        Ok(())
    }
}

/// Creates a symbolic link (Unix) or hard link (Windows) from source to target.
fn create_link(source: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target)
            .with_context(|| format!("Failed to create symlink from {} to {}", source.display(), target.display()))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(source, target)
            .or_else(|_| std::fs::hard_link(source, target))
            .or_else(|_| std::fs::copy(source, target).map(|_| ()))
            .with_context(|| format!("Failed to create link from {} to {}", source.display(), target.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn paths_with_infs_home_env() {
        // Use with_root directly to avoid race conditions with env vars
        let temp_dir = env::temp_dir().join("infs_test_home");
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        assert_eq!(paths.root, temp_dir);
        assert_eq!(paths.toolchains, temp_dir.join("toolchains"));
        assert_eq!(paths.bin, temp_dir.join("bin"));
        assert_eq!(paths.downloads, temp_dir.join("downloads"));
    }

    #[test]
    fn toolchain_dir_constructs_correct_path() {
        let temp_dir = env::temp_dir().join("infs_test_toolchain_dir");
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        assert_eq!(
            paths.toolchain_dir("0.1.0"),
            temp_dir.join("toolchains").join("0.1.0")
        );
    }

    #[test]
    fn default_file_path_is_correct() {
        let temp_dir = env::temp_dir().join("infs_test_default_file");
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        assert_eq!(paths.default_file(), temp_dir.join("default"));
    }

    #[test]
    fn download_path_constructs_correctly() {
        let temp_dir = env::temp_dir().join("infs_test_download");
        let paths = ToolchainPaths::with_root(temp_dir.clone());

        assert_eq!(
            paths.download_path("toolchain.zip"),
            temp_dir.join("downloads").join("toolchain.zip")
        );
        assert_eq!(
            paths.download_temp_path("toolchain.zip"),
            temp_dir.join("downloads").join("toolchain.zip.tmp")
        );
    }

    #[test]
    fn is_version_installed_returns_false_for_nonexistent() {
        let temp_dir = env::temp_dir().join("infs_test_installed");
        let paths = ToolchainPaths::with_root(temp_dir);

        assert!(!paths.is_version_installed("0.1.0"));
    }

    #[test]
    fn list_installed_versions_returns_empty_when_no_toolchains() {
        let temp_dir = env::temp_dir().join("infs_test_list_empty");
        let paths = ToolchainPaths::with_root(temp_dir);

        let versions = paths.list_installed_versions().expect("Should list versions");
        assert!(versions.is_empty());
    }
}
