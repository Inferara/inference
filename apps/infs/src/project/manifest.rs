//! Inference project manifest parsing and validation.
//!
//! This module handles the `Inference.toml` manifest file format, providing
//! parsing, validation, and serialization functionality.
//!
//! ## Manifest Format
//!
//! The Inference.toml file supports the following sections:
//!
//! ```toml
//! [package]
//! name = "myproject"
//! version = "0.1.0"
//! infc_version = "0.1.0"
//!
//! [dependencies]
//! # Future: package dependencies
//!
//! [build]
//! target = "wasm32"
//! optimize = "release"
//!
//! [verification]
//! output-dir = "proofs/"
//! ```
//!
//! ## Reserved Names
//!
//! Project names cannot use Inference keywords or problematic directory names.
//! See [`RESERVED_WORDS`] for the complete list.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Reserved words that cannot be used as project names.
///
/// Includes Inference language keywords and problematic directory names.
pub const RESERVED_WORDS: &[&str] = &[
    // Inference keywords
    "fn",
    "let",
    "mut",
    "if",
    "else",
    "match",
    "return",
    "type",
    "struct",
    "impl",
    "trait",
    "pub",
    "use",
    "mod",
    "ndet",
    "assume",
    "assert",
    "forall",
    "exists",
    "spec",
    "requires",
    "ensures",
    "invariant",
    "const",
    "enum",
    "loop",
    "break",
    "continue",
    "external",
    "unique",
    // Problematic directory/file names
    "src",
    "out",
    "target",
    "proofs",
    "tests",
    "self",
    "super",
    "crate",
];

/// The root manifest structure for `Inference.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InferenceToml {
    /// Package metadata section.
    pub package: Package,

    /// Project dependencies.
    #[serde(default, skip_serializing_if = "Dependencies::is_empty")]
    pub dependencies: Dependencies,

    /// Build configuration.
    #[serde(default, skip_serializing_if = "BuildConfig::is_default")]
    pub build: BuildConfig,

    /// Verification configuration for Rocq output.
    #[serde(default, skip_serializing_if = "VerificationConfig::is_default")]
    pub verification: VerificationConfig,
}

/// Package metadata in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Package {
    /// The project name.
    pub name: String,

    /// The project version (semver format).
    pub version: String,

    /// The infc compiler version used to create this project.
    #[serde(default = "default_infc_version")]
    pub infc_version: String,

    /// Optional project description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional list of authors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,

    /// Optional license identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

/// Project dependencies section.
///
/// Currently a placeholder for future package management support.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependencies {
    /// Map of dependency name to version specification.
    #[serde(flatten)]
    pub packages: HashMap<String, String>,
}

impl Dependencies {
    /// Returns true if there are no dependencies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

/// Build configuration section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildConfig {
    /// Target platform for compilation.
    #[serde(default = "default_target")]
    pub target: String,

    /// Optimization level.
    #[serde(default = "default_optimize")]
    pub optimize: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: default_target(),
            optimize: default_optimize(),
        }
    }
}

impl BuildConfig {
    /// Returns true if this is the default configuration.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.target == default_target() && self.optimize == default_optimize()
    }
}

/// Verification configuration for Rocq output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationConfig {
    /// Output directory for generated Rocq proofs.
    #[serde(default = "default_output_dir", rename = "output-dir")]
    pub output_dir: String,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
        }
    }
}

impl VerificationConfig {
    /// Returns true if this is the default configuration.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.output_dir == default_output_dir()
    }
}

/// Gets the infc version to use for new projects.
///
/// Tries to detect the installed infc version first by running `infc --version`.
/// If infc is not available or version detection fails, falls back to the infs
/// version (from `CARGO_PKG_VERSION`).
///
/// The detection is designed to be fast and non-blocking: it times out quickly
/// if infc is not responsive.
#[must_use]
pub fn detect_infc_version() -> String {
    try_detect_infc_version().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Attempts to detect the infc version by running `infc --version`.
///
/// Returns `None` if:
/// - infc is not found in PATH
/// - The command fails to execute
/// - The output cannot be parsed
/// - The version string is not valid
fn try_detect_infc_version() -> Option<String> {
    let output = Command::new("infc").arg("--version").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_infc_version_output(&stdout)
}

/// Parses the version from `infc --version` output.
///
/// Expected format: "infc X.Y.Z" (possibly with trailing newline or whitespace).
/// Returns the version string (e.g., "0.1.0") if parsing succeeds.
fn parse_infc_version_output(output: &str) -> Option<String> {
    let trimmed = output.trim();

    // Expected format: "infc X.Y.Z"
    let version = trimmed.strip_prefix("infc ")?.trim();

    // Validate that it looks like a version number
    if version.is_empty() {
        return None;
    }

    // Basic validation: should start with a digit
    if !version.chars().next()?.is_ascii_digit() {
        return None;
    }

    Some(version.to_string())
}

fn default_infc_version() -> String {
    detect_infc_version()
}

fn default_target() -> String {
    String::from("wasm32")
}

fn default_optimize() -> String {
    String::from("debug")
}

fn default_output_dir() -> String {
    String::from("proofs/")
}

impl InferenceToml {
    /// Creates a new manifest with the given project name.
    ///
    /// The version defaults to "0.1.0" and `infc_version` to the current toolchain version.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            package: Package {
                name: name.into(),
                version: String::from("0.1.0"),
                infc_version: default_infc_version(),
                description: None,
                authors: None,
                license: None,
            },
            dependencies: Dependencies::default(),
            build: BuildConfig::default(),
            verification: VerificationConfig::default(),
        }
    }

    /// Serializes the manifest to TOML format.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("Failed to serialize Inference.toml")
    }

    /// Writes the manifest to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let content = self.to_toml()?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write manifest: {}", path.display()))
    }
}

/// Validates a project name for use in Inference projects.
///
/// # Rules
///
/// - Must not be empty
/// - Must start with a letter or underscore
/// - Can only contain alphanumeric characters, underscores, and hyphens
/// - Must not be a reserved word
///
/// # Errors
///
/// Returns an error with a descriptive message if the name is invalid.
pub fn validate_project_name(name: &str) -> Result<()> {
    let Some(first_char) = name.chars().next() else {
        bail!("Project name cannot be empty");
    };

    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        bail!("Project name '{name}' must start with a letter or underscore");
    }

    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' {
            bail!(
                "Project name '{name}' contains invalid character '{ch}'. \
                 Only letters, numbers, underscores, and hyphens are allowed."
            );
        }
    }

    let name_lower = name.to_lowercase();
    if RESERVED_WORDS.contains(&name_lower.as_str()) {
        bail!(
            "Project name '{name}' is a reserved word. \
             Please choose a different name."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::Version;

    #[test]
    fn test_new_manifest_has_defaults() {
        let manifest = InferenceToml::new("myproject");
        assert_eq!(manifest.package.name, "myproject");
        assert_eq!(manifest.package.version, "0.1.0");
        // infc_version should be a valid semver (either detected or fallback)
        assert!(
            Version::parse(&manifest.package.infc_version).is_ok(),
            "infc_version should be valid semver"
        );
        assert!(manifest.package.description.is_none());
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.build.is_default());
        assert!(manifest.verification.is_default());
    }

    #[test]
    fn test_to_toml() {
        let manifest = InferenceToml::new("myproject");
        let output = manifest.to_toml().unwrap();
        assert!(output.contains("name = \"myproject\""));
        assert!(output.contains("version = \"0.1.0\""));
        assert!(output.contains("infc_version = \""));
    }

    #[test]
    fn test_dependencies_is_empty() {
        let deps = Dependencies::default();
        assert!(deps.is_empty());

        let mut deps = Dependencies::default();
        deps.packages
            .insert(String::from("std"), String::from("0.1"));
        assert!(!deps.is_empty());
    }

    #[test]
    fn test_build_config_is_default() {
        let config = BuildConfig::default();
        assert!(config.is_default());

        let config = BuildConfig {
            target: String::from("wasm64"),
            optimize: String::from("debug"),
        };
        assert!(!config.is_default());
    }

    #[test]
    fn test_verification_config_is_default() {
        let config = VerificationConfig::default();
        assert!(config.is_default());

        let config = VerificationConfig {
            output_dir: String::from("custom/"),
        };
        assert!(!config.is_default());
    }

    #[test]
    fn test_validate_project_name_valid() {
        assert!(validate_project_name("myproject").is_ok());
        assert!(validate_project_name("my_project").is_ok());
        assert!(validate_project_name("my-project").is_ok());
        assert!(validate_project_name("_private").is_ok());
        assert!(validate_project_name("Project123").is_ok());
    }

    #[test]
    fn test_validate_project_name_empty() {
        let result = validate_project_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_project_name_starts_with_number() {
        let result = validate_project_name("123project");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("start with"));
    }

    #[test]
    fn test_validate_project_name_invalid_chars() {
        let result = validate_project_name("my.project");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid character")
        );

        let result = validate_project_name("my project");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid character")
        );
    }

    #[test]
    fn test_validate_project_name_reserved_keywords() {
        for &word in &["fn", "let", "struct", "type", "return", "if", "else"] {
            let result = validate_project_name(word);
            assert!(result.is_err(), "Expected '{word}' to be rejected");
            assert!(result.unwrap_err().to_string().contains("reserved"));
        }
    }

    #[test]
    fn test_validate_project_name_reserved_directories() {
        for &word in &["src", "target", "proofs", "tests", "out"] {
            let result = validate_project_name(word);
            assert!(result.is_err(), "Expected '{word}' to be rejected");
            assert!(result.unwrap_err().to_string().contains("reserved"));
        }
    }

    #[test]
    fn test_validate_project_name_reserved_case_insensitive() {
        let result = validate_project_name("FN");
        assert!(result.is_err());

        let result = validate_project_name("Struct");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_infc_version_output_valid() {
        assert_eq!(
            parse_infc_version_output("infc 0.1.0"),
            Some("0.1.0".to_string())
        );
        assert_eq!(
            parse_infc_version_output("infc 1.2.3\n"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            parse_infc_version_output("infc 10.20.30\r\n"),
            Some("10.20.30".to_string())
        );
        assert_eq!(
            parse_infc_version_output("infc 0.0.1-alpha"),
            Some("0.0.1-alpha".to_string())
        );
    }

    #[test]
    fn test_parse_infc_version_output_invalid() {
        assert_eq!(parse_infc_version_output(""), None);
        assert_eq!(parse_infc_version_output("infc"), None);
        assert_eq!(parse_infc_version_output("infc "), None);
        assert_eq!(parse_infc_version_output("other 0.1.0"), None);
        assert_eq!(parse_infc_version_output("0.1.0"), None);
        assert_eq!(parse_infc_version_output("infc not-a-version"), None);
    }

    #[test]
    fn test_detect_infc_version_returns_valid_semver() {
        let version = detect_infc_version();
        assert!(!version.is_empty());
        // Should start with a digit (valid version format)
        assert!(
            version.chars().next().unwrap().is_ascii_digit(),
            "Version should start with a digit: {version}"
        );
    }
}
