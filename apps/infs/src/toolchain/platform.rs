#![warn(clippy::pedantic)]

//! Platform detection for the infs toolchain.
//!
//! This module provides OS and architecture detection to determine which
//! toolchain binaries to download for the current system.
//!
//! ## Supported Platforms
//!
//! - Linux `x86_64` (`linux-x64`)
//! - macOS ARM64 (`macos-arm64`)
//! - Windows `x86_64` (`windows-x64`)

use anyhow::{bail, Result};
use std::fmt;

/// Represents a supported platform for toolchain binaries.
///
/// Each variant corresponds to a specific OS and architecture combination
/// that has pre-built toolchain binaries available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Platform {
    /// Linux on `x86_64` architecture
    LinuxX64,
    /// macOS on ARM64 (Apple Silicon) architecture
    MacosArm64,
    /// Windows on `x86_64` architecture
    WindowsX64,
}

impl Platform {
    /// Detects the current platform based on compile-time configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the current OS/architecture combination is not supported.
    ///
    /// # Examples
    ///
    /// ```
    /// use infs::toolchain::Platform;
    ///
    /// let platform = Platform::detect()?;
    /// println!("Running on: {}", platform.as_str());
    /// ```
    pub fn detect() -> Result<Self> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Ok(Self::LinuxX64);
        }

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Ok(Self::MacosArm64);
        }

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            return Ok(Self::WindowsX64);
        }

        #[allow(unreachable_code)]
        {
            bail!(
                "Unsupported platform: {} on {}. \
                 Supported platforms are: linux-x64, macos-arm64, windows-x64",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
        }
    }

    /// Returns the platform identifier string used in manifest URLs and file names.
    ///
    /// These strings match the naming convention used in the release manifest.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX64 => "linux-x64",
            Self::MacosArm64 => "macos-arm64",
            Self::WindowsX64 => "windows-x64",
        }
    }

    /// Returns the executable file extension for this platform.
    ///
    /// Returns `.exe` on Windows, empty string on Unix platforms.
    #[must_use]
    pub fn executable_extension(self) -> &'static str {
        match self {
            Self::WindowsX64 => ".exe",
            Self::LinuxX64 | Self::MacosArm64 => "",
        }
    }

    /// Returns whether this platform is Windows.
    #[must_use]
    pub fn is_windows(self) -> bool {
        matches!(self, Self::WindowsX64)
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_as_str_returns_expected_values() {
        assert_eq!(Platform::LinuxX64.as_str(), "linux-x64");
        assert_eq!(Platform::MacosArm64.as_str(), "macos-arm64");
        assert_eq!(Platform::WindowsX64.as_str(), "windows-x64");
    }

    #[test]
    fn platform_display_matches_as_str() {
        assert_eq!(format!("{}", Platform::LinuxX64), "linux-x64");
        assert_eq!(format!("{}", Platform::MacosArm64), "macos-arm64");
        assert_eq!(format!("{}", Platform::WindowsX64), "windows-x64");
    }

    #[test]
    fn executable_extension_correct_for_each_platform() {
        assert_eq!(Platform::LinuxX64.executable_extension(), "");
        assert_eq!(Platform::MacosArm64.executable_extension(), "");
        assert_eq!(Platform::WindowsX64.executable_extension(), ".exe");
    }

    #[test]
    fn is_windows_only_true_for_windows() {
        assert!(!Platform::LinuxX64.is_windows());
        assert!(!Platform::MacosArm64.is_windows());
        assert!(Platform::WindowsX64.is_windows());
    }

    #[test]
    fn detect_returns_platform_on_supported_system() {
        let result = Platform::detect();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert!(matches!(result, Ok(Platform::LinuxX64)));

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert!(matches!(result, Ok(Platform::MacosArm64)));

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert!(matches!(result, Ok(Platform::WindowsX64)));
    }
}
