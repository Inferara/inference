#![warn(clippy::pedantic)]

//! Release manifest handling for the infs toolchain.
//!
//! This module provides functionality for fetching and parsing the toolchain
//! release manifest, which contains information about available versions
//! and download URLs.
//!
//! ## Manifest URL
//!
//! The default manifest URL is `https://inference-lang.org/releases/manifest.json`.
//! This can be overridden by setting the `INFS_MANIFEST_URL` environment variable.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::Platform;

/// Environment variable to override the manifest URL.
pub const MANIFEST_URL_ENV: &str = "INFS_MANIFEST_URL";

/// Default URL for the release manifest.
pub const DEFAULT_MANIFEST_URL: &str = "https://inference-lang.org/releases/manifest.json";

/// Release manifest containing available toolchain versions.
///
/// The manifest is a JSON document that lists all available toolchain versions
/// along with their platform-specific download URLs and checksums.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ReleaseManifest {
    /// Schema version of the manifest format.
    pub schema_version: u32,
    /// The latest stable toolchain version.
    pub latest_stable: String,
    /// Latest version of the `infs` CLI binary itself.
    #[serde(default)]
    pub latest_infs: Option<String>,
    /// List of all available toolchain versions.
    pub versions: Vec<VersionInfo>,
    /// Platform-specific artifacts for the `infs` CLI binary.
    #[serde(default)]
    pub infs_artifacts: Vec<PlatformArtifact>,
}

/// Information about a specific toolchain version.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VersionInfo {
    /// The version string (e.g., "0.1.0").
    pub version: String,
    /// Release date in ISO 8601 format (e.g., "2024-01-15").
    pub date: String,
    /// Platform-specific artifacts for this version.
    pub platforms: Vec<PlatformArtifact>,
}

/// Platform-specific download artifact.
#[derive(Debug, Clone, Deserialize)]
pub struct PlatformArtifact {
    /// Platform identifier (e.g., "linux-x64", "macos-arm64", "windows-x64").
    pub platform: String,
    /// Download URL for the artifact.
    pub url: String,
    /// SHA256 checksum of the artifact.
    pub sha256: String,
    /// Size of the artifact in bytes.
    pub size: u64,
}

#[allow(dead_code)]
impl ReleaseManifest {
    /// Finds version information for a specific version string.
    ///
    /// Returns `None` if the version is not found in the manifest.
    #[must_use]
    pub fn find_version(&self, version: &str) -> Option<&VersionInfo> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// Returns the latest stable version information.
    ///
    /// Returns `None` if the latest stable version is not found in the manifest.
    #[must_use]
    pub fn latest_stable_version(&self) -> Option<&VersionInfo> {
        self.find_version(&self.latest_stable)
    }

    /// Returns all available version strings.
    #[must_use]
    pub fn available_versions(&self) -> Vec<&str> {
        self.versions.iter().map(|v| v.version.as_str()).collect()
    }

    /// Finds the infs CLI artifact for the given platform.
    #[must_use]
    pub fn find_infs_artifact(&self, platform: Platform) -> Option<&PlatformArtifact> {
        self.infs_artifacts
            .iter()
            .find(|a| a.platform == platform.as_str())
    }
}

impl VersionInfo {
    /// Finds the artifact for a specific platform.
    ///
    /// Returns `None` if no artifact exists for the given platform.
    #[must_use]
    pub fn find_artifact(&self, platform: Platform) -> Option<&PlatformArtifact> {
        self.platforms
            .iter()
            .find(|a| a.platform == platform.as_str())
    }
}

/// Returns the manifest URL, checking the environment variable first.
#[must_use]
pub fn manifest_url() -> String {
    std::env::var(MANIFEST_URL_ENV).unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_string())
}

/// Fetches the release manifest from the configured URL.
///
/// # Errors
///
/// Returns an error if:
/// - The network request fails
/// - The response cannot be parsed as JSON
/// - The manifest schema is invalid
pub async fn fetch_manifest() -> Result<ReleaseManifest> {
    let url = manifest_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch manifest from {url}"))?;

    if !response.status().is_success() {
        bail!(
            "Failed to fetch manifest: HTTP {} from {url}",
            response.status()
        );
    }

    let text = response
        .text()
        .await
        .with_context(|| format!("Failed to read manifest response from {url}"))?;

    let manifest: ReleaseManifest = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse manifest JSON from {url}"))?;

    Ok(manifest)
}

/// Fetches the release manifest and finds the artifact for a specific version and platform.
///
/// If `version` is `None` or "latest", returns the latest stable version's artifact.
///
/// # Errors
///
/// Returns an error if:
/// - The manifest cannot be fetched
/// - The specified version is not found
/// - No artifact exists for the current platform
pub async fn fetch_artifact(version: Option<&str>, platform: Platform) -> Result<(String, PlatformArtifact)> {
    let manifest = fetch_manifest().await?;

    let version_str = match version {
        None | Some("latest") => manifest.latest_stable.clone(),
        Some(v) => v.to_string(),
    };

    let version_info = manifest
        .find_version(&version_str)
        .with_context(|| format!("Version {version_str} not found in manifest"))?;

    let artifact = version_info
        .find_artifact(platform)
        .with_context(|| format!("No artifact found for platform {platform} in version {version_str}"))?
        .clone();

    Ok((version_str, artifact))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest_json() -> &'static str {
        r#"{
            "schema_version": 1,
            "latest_stable": "0.2.0",
            "latest_infs": "0.2.0",
            "versions": [
                {
                    "version": "0.1.0",
                    "date": "2024-01-01",
                    "platforms": [
                        {
                            "platform": "linux-x64",
                            "url": "https://example.com/0.1.0/linux-x64.zip",
                            "sha256": "abc123",
                            "size": 1000
                        }
                    ]
                },
                {
                    "version": "0.2.0",
                    "date": "2024-02-01",
                    "platforms": [
                        {
                            "platform": "linux-x64",
                            "url": "https://example.com/0.2.0/linux-x64.zip",
                            "sha256": "def456",
                            "size": 2000
                        },
                        {
                            "platform": "macos-arm64",
                            "url": "https://example.com/0.2.0/macos-arm64.zip",
                            "sha256": "ghi789",
                            "size": 2100
                        }
                    ]
                }
            ],
            "infs_artifacts": [
                {
                    "platform": "linux-x64",
                    "url": "https://example.com/infs/linux-x64.zip",
                    "sha256": "infs123",
                    "size": 500
                }
            ]
        }"#
    }

    #[test]
    fn parse_manifest_json() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.latest_stable, "0.2.0");
        assert_eq!(manifest.versions.len(), 2);
    }

    #[test]
    fn find_version_returns_correct_info() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        let version = manifest.find_version("0.1.0").expect("Should find version");
        assert_eq!(version.version, "0.1.0");
        assert_eq!(version.date, "2024-01-01");
    }

    #[test]
    fn find_version_returns_none_for_missing() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        assert!(manifest.find_version("9.9.9").is_none());
    }

    #[test]
    fn latest_stable_version_returns_correct_info() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        let version = manifest
            .latest_stable_version()
            .expect("Should find latest stable");
        assert_eq!(version.version, "0.2.0");
    }

    #[test]
    fn find_artifact_returns_correct_platform() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        let version = manifest.find_version("0.2.0").expect("Should find version");
        let artifact = version
            .find_artifact(Platform::LinuxX64)
            .expect("Should find artifact");

        assert_eq!(artifact.platform, "linux-x64");
        assert_eq!(artifact.sha256, "def456");
    }

    #[test]
    fn find_artifact_returns_none_for_missing_platform() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        let version = manifest.find_version("0.1.0").expect("Should find version");
        assert!(version.find_artifact(Platform::WindowsX64).is_none());
    }

    #[test]
    fn available_versions_returns_all() {
        let manifest: ReleaseManifest =
            serde_json::from_str(sample_manifest_json()).expect("Should parse manifest");

        let versions = manifest.available_versions();
        assert_eq!(versions, vec!["0.1.0", "0.2.0"]);
    }

    #[test]
    fn manifest_url_uses_env_when_set() {
        unsafe { std::env::set_var(MANIFEST_URL_ENV, "https://custom.example.com/manifest.json") };
        assert_eq!(manifest_url(), "https://custom.example.com/manifest.json");
        unsafe { std::env::remove_var(MANIFEST_URL_ENV) };
    }

    #[test]
    fn manifest_url_uses_default_when_env_not_set() {
        unsafe { std::env::remove_var(MANIFEST_URL_ENV) };
        assert_eq!(manifest_url(), DEFAULT_MANIFEST_URL);
    }
}
