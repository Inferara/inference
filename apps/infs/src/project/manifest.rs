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
//! [wasm-dependencies]
//! # Logical module name -> location of a compiled `.wasm` module.
//! # The logical name is what source refers to via `use { f } from <name>;`.
//! arith = { path = "libs/arith.wasm" }
//!
//! [build]
//! target = "wasm32"
//! optimize = "release"
//! mode = "compile"        # "compile" (executable) or "proof" (Rocq specs)
//!
//! [verification]
//! output-dir = "proofs/"  # honored only in proof mode
//! ```
//!
//! ## Reserved Names
//!
//! Project names cannot use Inference keywords or problematic directory names.
//! See [`RESERVED_WORDS`] for the complete list.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The conventional manifest file name for an Inference project.
pub const MANIFEST_FILE_NAME: &str = "Inference.toml";

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

    /// External `.wasm` module dependencies, keyed by logical module name.
    #[serde(
        rename = "wasm-dependencies",
        default,
        skip_serializing_if = "WasmDependencies::is_empty"
    )]
    pub wasm_dependencies: WasmDependencies,

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

/// External `.wasm` module dependencies, keyed by logical module name.
///
/// Each entry maps a logical name — the identifier source refers to in
/// `use { f } from <name>;` — to the location of a compiled `.wasm` module.
/// These declarations are the highest-priority source feeding the compiler's
/// module resolver; `-L` search directories and `INFERENCE_*` environment
/// directories act as overrides only when a logical name is *not* declared here.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmDependencies {
    /// Map of logical module name to its location entry.
    #[serde(flatten)]
    pub modules: HashMap<String, WasmDependency>,
}

impl WasmDependencies {
    /// Returns true if no `.wasm` dependencies are declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

/// Validates a `[wasm-dependencies]` key against the logical-module-name grammar.
///
/// A logical name is one or more `::`-joined segments, each a non-empty ASCII
/// identifier (`[A-Za-z_][A-Za-z0-9_]*`). This is the same name source refers to
/// in `use { f } from <name>;`. Rejecting any other shape — in particular a key
/// containing `=` — keeps the `infs build` → `infc --wasm-dep <name>=<path>`
/// forwarding unambiguous, since the receiver splits on the first `=`.
///
/// # Errors
///
/// Returns an error naming the offending key when it is not a well-formed
/// logical name.
pub fn validate_wasm_dependency_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("invalid [wasm-dependencies] key: the module name is empty");
    }
    if key.contains('=') {
        bail!(
            "invalid [wasm-dependencies] key `{key}`: a module name cannot contain `=`"
        );
    }

    let segments: Vec<&str> = key.split("::").collect();
    for segment in &segments {
        if !is_logical_name_segment(segment) {
            bail!(
                "invalid [wasm-dependencies] key `{key}`: `{segment}` is not a valid \
                 module-name segment (expected `::`-joined ASCII identifiers)"
            );
        }
    }
    Ok(())
}

/// Returns true when `segment` is a non-empty ASCII identifier:
/// the first character is a letter or `_`, the rest are alphanumeric or `_`.
fn is_logical_name_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// The location of a single external `.wasm` module dependency.
///
/// Only a filesystem `path` is supported today. The entry is a table — not a
/// bare string — so future producers (version pins, registries) can add fields
/// without a breaking change to the manifest format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WasmDependency {
    /// Filesystem path to the compiled `.wasm` module, relative to the manifest.
    pub path: String,
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

    /// Compilation mode: `"compile"` (executable WASM) or `"proof"` (specs
    /// preserved for Rocq translation). Defaults to `"compile"`.
    ///
    /// This is an independent axis from [`optimize`](Self::optimize) (artifact
    /// kind vs optimization level) and is deliberately a validated `String`
    /// rather than a serde enum: the two-value axis is already modelled by
    /// `commands::build::BuildMode` (infs) and `CliMode` (infc), and a third
    /// representation would be a fourth source of truth. The string is mapped
    /// to `BuildMode` at the single forwarding site in `commands::build`.
    /// Validated case-sensitively on load (see [`InferenceToml::from_toml`]).
    #[serde(default = "default_mode")]
    pub mode: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: default_target(),
            optimize: default_optimize(),
            mode: default_mode(),
        }
    }
}

impl BuildConfig {
    /// Returns true if this is the default configuration.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.target == default_target()
            && self.optimize == default_optimize()
            && self.mode == default_mode()
    }

    /// Validates the `mode` field, accepting only `"compile"` or `"proof"`
    /// (case-sensitive — TOML config values are conventionally lowercase, and
    /// matching the exact `infc --mode` flag spelling avoids surprising
    /// near-misses like `"Proof"`).
    ///
    /// # Errors
    ///
    /// Returns an error naming the field and the allowed values when `mode` is
    /// neither `"compile"` nor `"proof"`.
    fn validate(&self) -> Result<()> {
        if self.mode != "compile" && self.mode != "proof" {
            bail!(
                "Invalid `[build] mode` value `{}`: expected `compile` or `proof`.",
                self.mode
            );
        }
        Ok(())
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

    /// Normalizes the configured `output-dir` into a relative [`PathBuf`]
    /// confined to the project root, suitable for forwarding to
    /// `infc --out-dir`.
    ///
    /// The raw manifest string (e.g. `"proofs/"`) is parsed through `PathBuf`
    /// and validated component-by-component so that only ordinary, project-
    /// relative subdirectories are accepted. A trailing separator is dropped
    /// (`"proofs/"` → `proofs`) and `.` segments are skipped (`"./proofs"` →
    /// `proofs`).
    ///
    /// The `output-dir` is a project-relative configuration: artifacts must
    /// land inside the project root so the `<root>/out`-style contract holds and
    /// nothing is written to locations outside VCS control. Anything that could
    /// point elsewhere is rejected:
    ///
    /// - **Root / absolute** (`/proofs`, `C:\proofs`): escapes the root.
    /// - **`..` parent traversal** (`../proofs`, `a/../b`): could climb out of
    ///   the root. Even a `..` that happens to resolve back inside is rejected —
    ///   resolving it would be symlink-unsound and buys nothing.
    /// - **Drive/UNC prefix** (`C:proofs`, `\\server\share`): on Windows these
    ///   are drive-relative or network paths that escape the project root. Such
    ///   prefixes only parse as a `Prefix` component on Windows; on unix
    ///   `C:proofs` is simply an ordinary directory name and is accepted as-is.
    ///
    /// # Errors
    ///
    /// Returns a remediation-style error (naming the offending value) when
    /// `output-dir` is empty, normalizes to an empty path, or contains a root,
    /// absolute, `..`, or drive/UNC component.
    pub fn normalized_output_dir(&self) -> Result<PathBuf> {
        let raw = self.output_dir.trim();
        if raw.is_empty() {
            bail!("`[verification] output-dir` must not be empty.");
        }

        let path = PathBuf::from(raw);
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                Component::ParentDir => bail!(
                    "`[verification] output-dir` must not contain `..` (got `{}`); \
                     it must stay inside the project root.",
                    self.output_dir
                ),
                Component::RootDir => bail!(
                    "`[verification] output-dir` must be a relative path (got `{}`); \
                     absolute paths would place artifacts outside the project root.",
                    self.output_dir
                ),
                Component::Prefix(_) => bail!(
                    "`[verification] output-dir` must not contain a drive or network \
                     prefix (got `{}`); it must stay inside the project root.",
                    self.output_dir
                ),
            }
        }

        if normalized.as_os_str().is_empty() {
            bail!(
                "`[verification] output-dir` `{}` normalizes to an empty path.",
                self.output_dir
            );
        }
        Ok(normalized)
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

fn default_mode() -> String {
    String::from("compile")
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
            wasm_dependencies: WasmDependencies::default(),
            build: BuildConfig::default(),
            verification: VerificationConfig::default(),
        }
    }

    /// Loads and parses a manifest from a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or is not valid
    /// `Inference.toml`.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;
        Self::from_toml(&content)
    }

    /// Resolves every `[wasm-dependencies]` entry to an absolute path.
    ///
    /// Each entry's `path` is interpreted relative to `base_dir` (the directory
    /// containing the manifest), then made absolute via [`Path::join`]. Entries
    /// already absolute are returned unchanged. The result preserves the logical
    /// name so the resolver can key on it.
    ///
    /// Each key is validated against the logical-module-name grammar
    /// ([`validate_wasm_dependency_key`]) so a malformed name — in particular one
    /// containing `=` — never silently corrupts the `--wasm-dep <name>=<path>`
    /// forwarding to `infc`.
    ///
    /// The returned order is sorted by logical name for determinism.
    ///
    /// # Errors
    ///
    /// Returns an error if any `[wasm-dependencies]` key is not a well-formed
    /// logical module name.
    pub fn resolved_wasm_dependencies(
        &self,
        base_dir: &Path,
    ) -> Result<Vec<(String, std::path::PathBuf)>> {
        let mut resolved: Vec<(String, std::path::PathBuf)> =
            Vec::with_capacity(self.wasm_dependencies.modules.len());
        for (name, dep) in &self.wasm_dependencies.modules {
            validate_wasm_dependency_key(name)?;
            resolved.push((name.clone(), base_dir.join(&dep.path)));
        }
        resolved.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(resolved)
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

    /// Parses a manifest from a TOML string.
    ///
    /// Missing optional sections (`[dependencies]`, `[build]`,
    /// `[verification]`) are filled in with their defaults; absent fields
    /// within present sections likewise default. Only `[package]` (with at
    /// least `name` and `version`) is required. After structural parsing, the
    /// `[build] mode` value is validated against its allowed set.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid TOML, does not match the
    /// manifest schema (e.g. `[package]` is missing), or carries an invalid
    /// `[build] mode` value.
    pub fn from_toml(s: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(s).context("Failed to parse Inference.toml")?;
        manifest.build.validate()?;
        Ok(manifest)
    }

    /// Reads and parses a manifest from a file on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or its contents do not
    /// parse as a valid manifest. The error context names the offending path.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;
        Self::from_toml(&content)
            .with_context(|| format!("Invalid manifest: {}", path.display()))
    }
}

/// Walks `start` and its ancestors looking for an `Inference.toml`.
///
/// The start directory is canonicalized once (for symlink stability and
/// reliable termination), then each ancestor is checked in order. The
/// **nearest** ancestor containing a manifest wins (cargo convention: a nested
/// project's manifest shadows an outer one by design). The walk stops at the
/// filesystem root.
///
/// Returns the absolute path to the discovered `Inference.toml`.
///
/// # Errors
///
/// Returns a remediation-style error if `start` cannot be canonicalized or no
/// manifest exists in `start` or any ancestor.
pub fn discover_manifest(start: &Path) -> Result<PathBuf> {
    let canonical = start
        .canonicalize()
        .with_context(|| format!("Failed to resolve directory: {}", start.display()))?;

    for dir in canonical.ancestors() {
        let candidate = dir.join(MANIFEST_FILE_NAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!(
        "No {MANIFEST_FILE_NAME} found in {} or any parent directory. \
         Run `infs new <name>` to create a project, or `infs init` to \
         initialize the current directory, or pass a source file path \
         (`infs build path/to/file.inf`).",
        canonical.display()
    )
}

/// Locates the nearest `Inference.toml` by walking up from `start`.
///
/// `start` may be a file (e.g. the source being compiled) or a directory; the
/// search begins at `start`'s directory and ascends to the filesystem root,
/// returning the first directory that contains an `Inference.toml`. Returns
/// `None` when no manifest is found — a bare file compiled outside any project
/// is a valid, manifest-free build.
#[must_use]
pub fn find_manifest_dir(start: &Path) -> Option<std::path::PathBuf> {
    let mut dir = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    while let Some(current) = dir {
        if current.join(MANIFEST_FILE_NAME).is_file() {
            return Some(current.to_path_buf());
        }
        dir = current.parent();
    }
    None
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
    use assert_fs::prelude::*;
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
            mode: default_mode(),
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

    #[test]
    fn build_config_default_mode_is_compile() {
        assert_eq!(BuildConfig::default().mode, "compile");
        assert_eq!(default_mode(), "compile");
    }

    #[test]
    fn is_default_requires_compile_mode() {
        let mut config = BuildConfig::default();
        assert!(config.is_default());
        config.mode = String::from("proof");
        assert!(
            !config.is_default(),
            "proof mode must not be reported as the default config"
        );
    }

    #[test]
    fn test_new_manifest_has_no_wasm_dependencies() {
        let manifest = InferenceToml::new("myproject");
        assert!(manifest.wasm_dependencies.is_empty());
    }

    #[test]
    fn test_wasm_dependencies_default_is_omitted_from_toml() {
        // An empty `[wasm-dependencies]` table must not be serialized — a fresh
        // manifest stays minimal.
        let manifest = InferenceToml::new("myproject");
        let output = manifest.to_toml().unwrap();
        assert!(
            !output.contains("wasm-dependencies"),
            "empty wasm-dependencies should be skipped, got:\n{output}"
        );
    }

    #[test]
    fn from_toml_parses_explicit_compile_mode() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
mode = "compile"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert_eq!(manifest.build.mode, "compile");
    }

    #[test]
    fn from_toml_parses_explicit_proof_mode() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
mode = "proof"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert_eq!(manifest.build.mode, "proof");
    }

    #[test]
    fn from_toml_defaults_absent_mode_to_compile() {
        // [build] present but no `mode` key; and [build] entirely absent.
        let with_build = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
optimize = "release"
"#;
        assert_eq!(
            InferenceToml::from_toml(with_build).unwrap().build.mode,
            "compile"
        );

        let no_build = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"
"#;
        assert_eq!(
            InferenceToml::from_toml(no_build).unwrap().build.mode,
            "compile"
        );
    }

    #[test]
    fn test_parse_wasm_dependencies_table() {
        let content = r#"
            [package]
            name = "demo"
            version = "0.1.0"
            infc_version = "0.1.0"

            [wasm-dependencies]
            arith = { path = "libs/arith.wasm" }
            crypto = { path = "vendor/sha256.wasm" }
        "#;
        let manifest = InferenceToml::from_toml(content).expect("should parse");

        assert_eq!(manifest.wasm_dependencies.modules.len(), 2);
        assert_eq!(
            manifest.wasm_dependencies.modules["arith"].path,
            "libs/arith.wasm"
        );
        assert_eq!(
            manifest.wasm_dependencies.modules["crypto"].path,
            "vendor/sha256.wasm"
        );
    }

    #[test]
    fn from_toml_rejects_invalid_mode() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
mode = "release"
"#;
        let err = InferenceToml::from_toml(src).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("mode")
                && msg.contains("compile")
                && msg.contains("proof")
                && msg.contains("release"),
            "error must name the field, the offending value, and the allowed set, got: {msg}"
        );
    }

    #[test]
    fn from_toml_mode_is_case_sensitive() {
        // `"Proof"` is a near-miss: rejected, not silently accepted. This pins
        // the documented case-sensitivity decision.
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
mode = "Proof"
"#;
        assert!(
            InferenceToml::from_toml(src).is_err(),
            "mode validation is case-sensitive; `Proof` must be rejected"
        );
    }

    #[test]
    fn normalized_output_dir_strips_trailing_separator() {
        let config = VerificationConfig {
            output_dir: String::from("proofs/"),
        };
        assert_eq!(config.normalized_output_dir().unwrap(), PathBuf::from("proofs"));
    }

    #[test]
    fn normalized_output_dir_accepts_nested_relative() {
        let config = VerificationConfig {
            output_dir: String::from("build/artifacts"),
        };
        assert_eq!(
            config.normalized_output_dir().unwrap(),
            PathBuf::from("build").join("artifacts")
        );
    }

    #[test]
    fn normalized_output_dir_rejects_absolute() {
        // Use a platform-appropriate absolute path. On unix this is a RootDir
        // component ("relative" remediation); on Windows it is a drive Prefix
        // component ("prefix" remediation). Either way it must be rejected and
        // name the offending value.
        let abs = if cfg!(windows) {
            r"C:\proofs"
        } else {
            "/var/proofs"
        };
        let config = VerificationConfig {
            output_dir: String::from(abs),
        };
        let err = config.normalized_output_dir().unwrap_err();
        let msg = err.to_string();
        let expected_remediation = if cfg!(windows) { "prefix" } else { "relative" };
        assert!(
            msg.contains(expected_remediation) && msg.contains(abs),
            "absolute output-dir must be rejected naming the value, got: {msg}"
        );
    }

    #[test]
    fn normalized_output_dir_rejects_empty() {
        let config = VerificationConfig {
            output_dir: String::from("   "),
        };
        assert!(
            config.normalized_output_dir().is_err(),
            "blank output-dir must be rejected"
        );
    }

    #[test]
    fn normalized_output_dir_accepts_curdir_prefix() {
        let config = VerificationConfig {
            output_dir: String::from("./proofs"),
        };
        assert_eq!(
            config.normalized_output_dir().unwrap(),
            PathBuf::from("proofs"),
            "a leading `./` must be tolerated and stripped"
        );
    }

    #[test]
    fn normalized_output_dir_rejects_leading_parent_traversal() {
        let config = VerificationConfig {
            output_dir: String::from("../proofs"),
        };
        let err = config.normalized_output_dir().unwrap_err();
        assert!(
            err.to_string().contains("..") && err.to_string().contains("../proofs"),
            "leading `..` must be rejected naming the value, got: {err}"
        );
    }

    #[test]
    fn normalized_output_dir_rejects_trailing_parent_traversal() {
        let config = VerificationConfig {
            output_dir: String::from("proofs/../.."),
        };
        assert!(
            config.normalized_output_dir().is_err(),
            "`proofs/../..` climbs out of the root and must be rejected"
        );
    }

    #[test]
    fn normalized_output_dir_rejects_interior_parent_even_if_resolving_inside() {
        // `a/../b` resolves to `b` (inside the root), but we reject ANY `..`
        // rather than resolve it: resolution is symlink-unsound.
        let config = VerificationConfig {
            output_dir: String::from("a/../b"),
        };
        assert!(
            config.normalized_output_dir().is_err(),
            "any `..` must be rejected, even one that resolves inside the root"
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalized_output_dir_rejects_drive_relative_prefix() {
        // `C:proofs` is drive-relative (NOT absolute by Rust's definition) but
        // escapes the project root on Windows: it parses as a Prefix component.
        let config = VerificationConfig {
            output_dir: String::from("C:proofs"),
        };
        let err = config.normalized_output_dir().unwrap_err();
        assert!(
            err.to_string().contains("prefix") && err.to_string().contains("C:proofs"),
            "drive-relative prefix must be rejected naming the value, got: {err}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalized_output_dir_rejects_unc_path() {
        let config = VerificationConfig {
            output_dir: String::from(r"\\server\share\x"),
        };
        assert!(
            config.normalized_output_dir().is_err(),
            "a UNC network path must be rejected"
        );
    }

    #[cfg(unix)]
    #[test]
    fn normalized_output_dir_treats_drive_letter_as_plain_dirname_on_unix() {
        // On unix there is no Prefix component: `C:proofs` is a single ordinary
        // directory name (a colon is a legal filename character) and is kept
        // verbatim. This documents the platform difference.
        let config = VerificationConfig {
            output_dir: String::from("C:proofs"),
        };
        assert_eq!(
            config.normalized_output_dir().unwrap(),
            PathBuf::from("C:proofs"),
            "on unix `C:proofs` is a valid plain directory name"
        );
    }

    #[test]
    fn scaffolded_default_manifest_roundtrips_mode_compile() {
        // The typed model the scaffold writes (via InferenceToml::new) must load
        // back with mode == "compile". (The string scaffold template is covered
        // separately in scaffold.rs.)
        let dir = assert_fs::TempDir::new().unwrap();
        let path = dir.path().join(MANIFEST_FILE_NAME);
        InferenceToml::new("scaffolded").write_to_file(&path).unwrap();

        let loaded = InferenceToml::load(&path).unwrap();
        assert_eq!(loaded.build.mode, "compile");
        assert!(loaded.build.is_default());
    }

    #[test]
    fn from_toml_parses_full_manifest() {
        let src = r#"
[package]
name = "demo"
version = "1.2.3"
infc_version = "0.1.0"

[build]
target = "wasm32"
optimize = "release"

[verification]
output-dir = "custom/"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert_eq!(manifest.package.name, "demo");
        assert_eq!(manifest.package.version, "1.2.3");
        assert_eq!(manifest.package.infc_version, "0.1.0");
        assert_eq!(manifest.build.target, "wasm32");
        assert_eq!(manifest.build.optimize, "release");
        assert_eq!(manifest.verification.output_dir, "custom/");
    }

    #[test]
    fn from_toml_defaults_missing_sections() {
        // Only [package] is present; [build] and [verification] must default.
        let src = r#"
[package]
name = "minimal"
version = "0.1.0"
infc_version = "0.1.0"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert_eq!(manifest.package.name, "minimal");
        assert!(manifest.dependencies.is_empty());
        assert!(
            manifest.build.is_default(),
            "absent [build] must yield the default BuildConfig"
        );
        assert!(
            manifest.verification.is_default(),
            "absent [verification] must yield the default VerificationConfig"
        );
        assert_eq!(manifest.build.target, default_target());
        assert_eq!(manifest.build.optimize, default_optimize());
        assert_eq!(manifest.verification.output_dir, default_output_dir());
    }

    #[test]
    fn from_toml_defaults_missing_keys_within_sections() {
        // Present-but-partial [build]/[verification]: absent keys still default.
        let src = r#"
[package]
name = "partial"
version = "0.1.0"
infc_version = "0.1.0"

[build]
optimize = "release"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert_eq!(manifest.build.optimize, "release");
        assert_eq!(
            manifest.build.target,
            default_target(),
            "absent build.target must default"
        );
        assert_eq!(manifest.verification.output_dir, default_output_dir());
    }

    #[test]
    fn from_toml_rejects_malformed_toml() {
        let result = InferenceToml::from_toml("this is = = not valid toml");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Inference.toml"),
            "error should mention the manifest"
        );
    }

    #[test]
    fn from_toml_rejects_missing_package() {
        // [package] (and its required name/version) is mandatory.
        let src = r#"
[build]
target = "wasm32"
"#;
        let result = InferenceToml::from_toml(src);
        assert!(result.is_err(), "missing [package] must be rejected");
    }

    #[test]
    fn load_reads_manifest_from_disk() {
        let dir = assert_fs::TempDir::new().unwrap();
        let manifest_path = dir.path().join(MANIFEST_FILE_NAME);
        let manifest = InferenceToml::new("roundtrip");
        manifest.write_to_file(&manifest_path).unwrap();

        let loaded = InferenceToml::load(&manifest_path).unwrap();
        assert_eq!(loaded.package.name, "roundtrip");
        assert_eq!(loaded.package.version, "0.1.0");
    }

    #[test]
    fn load_errors_on_missing_file() {
        let dir = assert_fs::TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist.toml");
        let result = InferenceToml::load(&missing);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("read manifest"),
            "error should report the read failure"
        );
    }

    #[test]
    fn load_errors_on_invalid_contents() {
        let dir = assert_fs::TempDir::new().unwrap();
        let manifest_path = dir.path().join(MANIFEST_FILE_NAME);
        std::fs::write(&manifest_path, "not = = valid").unwrap();
        let result = InferenceToml::load(&manifest_path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Invalid manifest"),
            "error context should name the invalid manifest, got: {msg}"
        );
    }

    #[test]
    fn discover_manifest_finds_at_start_dir() {
        let dir = assert_fs::TempDir::new().unwrap();
        let manifest_path = dir.path().join(MANIFEST_FILE_NAME);
        std::fs::write(&manifest_path, "").unwrap();

        let found = discover_manifest(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), MANIFEST_FILE_NAME);
        // Canonicalize both sides: discover_manifest canonicalizes the start.
        assert_eq!(
            found.canonicalize().unwrap(),
            manifest_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_manifest_finds_in_ancestor() {
        let root = assert_fs::TempDir::new().unwrap();
        let manifest_path = root.path().join(MANIFEST_FILE_NAME);
        std::fs::write(&manifest_path, "").unwrap();

        let nested = root.path().join("src").join("deep").join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let found = discover_manifest(&nested).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            manifest_path.canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_manifest_nearest_ancestor_wins() {
        // Outer project contains an inner project; from inside the inner
        // project the inner manifest must shadow the outer one.
        let outer = assert_fs::TempDir::new().unwrap();
        std::fs::write(outer.path().join(MANIFEST_FILE_NAME), "").unwrap();

        let inner = outer.path().join("vendor").join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        let inner_manifest = inner.join(MANIFEST_FILE_NAME);
        std::fs::write(&inner_manifest, "").unwrap();

        let found = discover_manifest(&inner).unwrap();
        assert_eq!(
            found.canonicalize().unwrap(),
            inner_manifest.canonicalize().unwrap(),
            "nearest ancestor manifest must win"
        );
    }

    #[test]
    fn discover_manifest_errors_when_absent() {
        // A fresh temp dir with no manifest in it or (realistically) any
        // ancestor up to the temp root.
        let dir = assert_fs::TempDir::new().unwrap();
        let result = discover_manifest(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(MANIFEST_FILE_NAME) && msg.contains("infs new"),
            "error should mention the manifest and remediation, got: {msg}"
        );
    }

    #[test]
    fn discover_manifest_errors_on_nonexistent_start() {
        let dir = assert_fs::TempDir::new().unwrap();
        let missing = dir.path().join("no-such-dir");
        let result = discover_manifest(&missing);
        assert!(
            result.is_err(),
            "a non-existent start directory cannot be canonicalized"
        );
    }

    #[test]
    fn test_parse_manifest_without_wasm_dependencies() {
        // A manifest that predates the feature must still parse, with an empty
        // dependency set.
        let content = r#"
            [package]
            name = "demo"
            version = "0.1.0"
            infc_version = "0.1.0"
        "#;
        let manifest = InferenceToml::from_toml(content).expect("should parse");
        assert!(manifest.wasm_dependencies.is_empty());
    }

    #[test]
    fn test_wasm_dependencies_round_trip() {
        let content = r#"
            [package]
            name = "demo"
            version = "0.1.0"
            infc_version = "0.1.0"

            [wasm-dependencies]
            arith = { path = "libs/arith.wasm" }
        "#;
        let manifest = InferenceToml::from_toml(content).expect("should parse");
        let serialized = manifest.to_toml().expect("should serialize");
        let reparsed = InferenceToml::from_toml(&serialized).expect("should reparse");
        assert_eq!(manifest, reparsed);
        assert!(serialized.contains("wasm-dependencies"));
    }

    #[test]
    fn test_resolved_wasm_dependencies_joins_against_base_dir() {
        let content = r#"
            [package]
            name = "demo"
            version = "0.1.0"
            infc_version = "0.1.0"

            [wasm-dependencies]
            arith = { path = "libs/arith.wasm" }
            beta = { path = "vendor/beta.wasm" }
        "#;
        let manifest = InferenceToml::from_toml(content).expect("should parse");
        let base = Path::new("/projects/demo");

        let resolved = manifest
            .resolved_wasm_dependencies(base)
            .expect("valid keys resolve");

        // Sorted by logical name for determinism.
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].0, "arith");
        assert_eq!(resolved[0].1, base.join("libs/arith.wasm"));
        assert_eq!(resolved[1].0, "beta");
        assert_eq!(resolved[1].1, base.join("vendor/beta.wasm"));
    }

    #[test]
    fn validate_wasm_dependency_key_accepts_logical_names() {
        for key in ["arith", "crypto", "_priv", "a1", "crypto::sha256", "a::b::c"] {
            assert!(
                validate_wasm_dependency_key(key).is_ok(),
                "`{key}` should be a valid logical name"
            );
        }
    }

    #[test]
    fn validate_wasm_dependency_key_rejects_equals_bearing_keys() {
        // L1: a `=` in a key would corrupt the `--wasm-dep <name>=<path>`
        // forwarding, which splits on the first `=`. Reject it outright.
        let err = validate_wasm_dependency_key("arith=evil").unwrap_err();
        assert!(err.to_string().contains("cannot contain `=`"), "{err}");
    }

    #[test]
    fn validate_wasm_dependency_key_rejects_malformed_segments() {
        for bad in ["", "1arith", "a-b", "a/b", "a::", "::a", "a..b", "a b"] {
            assert!(
                validate_wasm_dependency_key(bad).is_err(),
                "`{bad}` should be rejected as an invalid logical name"
            );
        }
    }

    #[test]
    fn resolved_wasm_dependencies_rejects_an_invalid_key() {
        let content = r#"
            [package]
            name = "demo"
            version = "0.1.0"
            infc_version = "0.1.0"

            [wasm-dependencies]
            "bad=key" = { path = "libs/x.wasm" }
        "#;
        let manifest = InferenceToml::from_toml(content).expect("manifest parses");
        let err = manifest
            .resolved_wasm_dependencies(Path::new("/projects/demo"))
            .expect_err("an `=`-bearing key must be rejected");
        assert!(err.to_string().contains("bad=key"), "{err}");
    }

    #[test]
    fn test_resolved_wasm_dependencies_empty_when_none_declared() {
        let manifest = InferenceToml::new("demo");
        let resolved = manifest
            .resolved_wasm_dependencies(Path::new("/projects/demo"))
            .expect("no keys to validate");
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_find_manifest_dir_in_same_directory() {
        let temp = assert_fs::TempDir::new().unwrap();
        let manifest = temp.child(MANIFEST_FILE_NAME);
        manifest.write_str("[package]\nname = \"x\"\nversion = \"0.1.0\"\n").unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();

        let found = find_manifest_dir(source.path()).expect("manifest should be found");
        assert_eq!(found, temp.path());
    }

    #[test]
    fn test_find_manifest_dir_walks_up_from_nested_source() {
        let temp = assert_fs::TempDir::new().unwrap();
        let manifest = temp.child(MANIFEST_FILE_NAME);
        manifest.write_str("[package]\nname = \"x\"\nversion = \"0.1.0\"\n").unwrap();
        let nested = temp.child("src").child("deep");
        nested.create_dir_all().unwrap();
        let source = nested.child("main.inf");
        source.write_str("").unwrap();

        let found = find_manifest_dir(source.path()).expect("manifest should be found");
        assert_eq!(found, temp.path());
    }

    #[test]
    fn test_find_manifest_dir_returns_none_without_manifest() {
        let temp = assert_fs::TempDir::new().unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();
        assert!(find_manifest_dir(source.path()).is_none());
    }
}
