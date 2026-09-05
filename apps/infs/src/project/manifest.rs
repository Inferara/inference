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
//! wasm-features = []      # post-MVP WebAssembly proposals to opt into
//!
//! [build.wasm-opt]        # optional: post-build optimization of the executable
//! enabled = true          # table presence enables; set false to keep it off
//! level = "3"             # forwarded as -O<level>: "0".."4", "s", "z"
//! auto-install = false    # download wasm-opt automatically if it is missing
//!
//! [memory]                # linear memory of the emitted module
//! pages = 1               # 64 KiB pages; emitted as a fixed, non-growable size
//! stack-size = 65536      # shadow stack bytes, at the bottom of that memory
//!
//! [verification]
//! output-dir = "proofs/"  # honored only in proof mode
//! adopt-external-specs = false    # carry linked libraries' universal obligations
//! ```
//!
//! ## Unknown Keys
//!
//! Every table whose keys are a fixed schema rejects keys it does not know, so a
//! typo is a build error instead of a setting that silently does nothing. The
//! `toml` parser names the offending key and the fields it expected. Only
//! `[dependencies]` and `[wasm-dependencies]` accept arbitrary keys, because
//! there the keys *are* the data — they name dependencies.
//!
//! The trade-off is deliberate: an older `infs` reading a manifest that uses a
//! newer key fails rather than ignoring it. That matches how the compiler ABI
//! gate treats toolchain/manifest skew — an error, never a silent downgrade that
//! ships a differently-configured artifact than the manifest asked for.
//!
//! ## Reserved Names
//!
//! Project names cannot use Inference keywords or problematic directory names.
//! See [`RESERVED_WORDS`] for the complete list.

use anyhow::{Context, Result, bail};
use inference_compiler_interface::{
    MemoryLayout, MemoryLayoutSource, WasmFeatureName, WasmFeatureSource, resolve_wasm_features,
};
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
#[serde(deny_unknown_fields)]
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

    /// Linear memory configuration.
    ///
    /// A top-level table rather than a `[build]` key because it describes the
    /// artifact's shape rather than how the build runs, and because it is read in
    /// both compilation modes — a proof-mode `.v` describes frames laid out in
    /// exactly this memory.
    #[serde(default, skip_serializing_if = "MemoryConfig::is_default")]
    pub memory: MemoryConfig,

    /// Verification configuration for Rocq output.
    #[serde(default, skip_serializing_if = "VerificationConfig::is_default")]
    pub verification: VerificationConfig,
}

/// Package metadata in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
        bail!("invalid [wasm-dependencies] key `{key}`: a module name cannot contain `=`");
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
/// bare string — so future producers (version pins, registries) have somewhere to
/// add fields: a reader that knows a new field accepts entries carrying it, and
/// entries that omit it, without either side changing shape.
///
/// That compatibility runs forward only. Like every fixed-schema table here the
/// entry rejects unknown keys, so an `infs` predating a field refuses a manifest
/// that uses it rather than silently dropping it — the deliberate policy for
/// toolchain/manifest skew described at the module level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WasmDependency {
    /// Filesystem path to the compiled `.wasm` module, relative to the manifest.
    pub path: String,
}

/// Build configuration section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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

    /// Post-MVP WebAssembly proposals the emitted module opts into, named after
    /// the proposal (`"bulk-memory"`), never after an instruction.
    ///
    /// Empty (the default) means pure WebAssembly 1.0 output. Entries are kept as
    /// raw strings and resolved against the shared vocabulary by
    /// [`Self::resolved_wasm_features`]; validation runs on load, so a manifest
    /// that reached a caller has already been checked.
    ///
    /// Flipping this changes the instruction set of every artifact the project
    /// produces, in both compile and proof mode. That is why it lives in the
    /// versioned manifest rather than in a flag.
    #[serde(
        rename = "wasm-features",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wasm_features: Vec<String>,

    /// Optional `[build.wasm-opt]` sub-table. Absent means post-build
    /// optimization is off; present means on unless `enabled = false`.
    ///
    /// Declared last because a TOML sub-table must serialize after the scalar
    /// keys of its parent table, and `toml` emits struct fields in declaration
    /// order. Anything added below it would serialize *under* the
    /// `[build.wasm-opt]` header and reparent into that sub-table on the next
    /// parse.
    #[serde(rename = "wasm-opt", default, skip_serializing_if = "Option::is_none")]
    pub wasm_opt: Option<WasmOptConfig>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            target: default_target(),
            optimize: default_optimize(),
            mode: default_mode(),
            wasm_features: Vec::new(),
            wasm_opt: None,
        }
    }
}

impl BuildConfig {
    /// Returns true if this is the default configuration.
    ///
    /// A present `[build.wasm-opt]` table makes the config non-default even
    /// when the other fields are defaults: without this, a manifest whose only
    /// `[build]` content is `[build.wasm-opt]` would round-trip to nothing
    /// (the whole `[build]` table is skipped when `is_default()` holds).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.target == default_target()
            && self.optimize == default_optimize()
            && self.mode == default_mode()
            && self.wasm_features.is_empty()
            && self.wasm_opt.is_none()
    }

    /// The `wasm-features` entries resolved into the shared compiler vocabulary.
    ///
    /// Callers get typed features without knowing how the raw strings are spelled
    /// or validated. The resolution is the same call [`Self::validate`] makes on
    /// load, so this cannot disagree with what the loader accepted.
    ///
    /// # Errors
    ///
    /// Returns the diagnostic rejecting the first entry that is not a valid,
    /// not-yet-seen feature name. For a manifest that came from
    /// [`InferenceToml::from_toml`] this cannot fail — validation already ran. The
    /// fallible signature is for the other constructor: [`Self::wasm_features`] is
    /// a public `Vec<String>` field, so a test or tool that builds a
    /// `BuildConfig` in memory can populate it without ever passing through the
    /// loader, and this is where such a value is checked.
    pub fn resolved_wasm_features(&self) -> Result<Vec<WasmFeatureName>> {
        resolve_wasm_features(&self.wasm_features, WasmFeatureSource::Manifest)
            .map_err(|message| anyhow::anyhow!("{message}"))
    }

    /// Validates the `mode` field, accepting only `"compile"` or `"proof"`
    /// (case-sensitive — TOML config values are conventionally lowercase, and
    /// matching the exact `infc --mode` flag spelling avoids surprising
    /// near-misses like `"Proof"`), then the `wasm-features` entries and the
    /// `[build.wasm-opt]` sub-table.
    ///
    /// # Errors
    ///
    /// Returns an error naming the field and the allowed values when `mode` is
    /// neither `"compile"` nor `"proof"`, when a `wasm-features` entry is not a
    /// supported proposal name, or when `[build.wasm-opt]` is invalid.
    fn validate(&self) -> Result<()> {
        if self.mode != "compile" && self.mode != "proof" {
            bail!(
                "Invalid `[build] mode` value `{}`: expected `compile` or `proof`.",
                self.mode
            );
        }
        self.resolved_wasm_features()?;
        if let Some(wasm_opt) = &self.wasm_opt {
            wasm_opt.validate()?;
        }
        Ok(())
    }
}

/// The `[build.wasm-opt]` table: post-build optimization of the compile-mode
/// artifact via the external Binaryen `wasm-opt` binary. Table presence means
/// enabled unless `enabled = false`. Proof-mode artifacts are never optimized.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WasmOptConfig {
    /// Whether the optimizer runs. Defaults to `true` so that merely declaring
    /// `[build.wasm-opt]` turns the feature on.
    #[serde(default = "default_wasm_opt_enabled")]
    pub enabled: bool,

    /// Optimization level, forwarded as `-O<level>` to `wasm-opt`. One of
    /// [`WASM_OPT_LEVELS`]: `"0"`..`"4"`, `"s"`, `"z"`.
    #[serde(default = "default_wasm_opt_level")]
    pub level: String,

    /// Whether a missing `wasm-opt` is downloaded automatically at build time.
    ///
    /// Defaults to `false`: an absent binary is a hard error with install
    /// remediation, so a build never reaches out to the network unless the
    /// project opts in. Set `true` to have `infs` provision the pinned,
    /// checksum-verified Binaryen (the same install `infs component add
    /// wasm-opt` performs) on first use. The opt-in is recorded in the versioned
    /// manifest — there are no interactive prompts.
    #[serde(rename = "auto-install", default)]
    pub auto_install: bool,
}

/// The `-O<level>` values accepted by `[build.wasm-opt] level`, matching the
/// levels `wasm-opt` itself understands: numeric `0`–`4`, plus the size-biased
/// `s` and `z`.
pub const WASM_OPT_LEVELS: &[&str] = &["0", "1", "2", "3", "4", "s", "z"];

impl Default for WasmOptConfig {
    fn default() -> Self {
        Self {
            enabled: default_wasm_opt_enabled(),
            level: default_wasm_opt_level(),
            auto_install: false,
        }
    }
}

impl WasmOptConfig {
    /// Validates the `level` field against [`WASM_OPT_LEVELS`].
    ///
    /// # Errors
    ///
    /// Returns an error naming the offending value and the allowed set when
    /// `level` is not one of the accepted `-O<level>` values.
    fn validate(&self) -> Result<()> {
        if !WASM_OPT_LEVELS.contains(&self.level.as_str()) {
            bail!(
                "Invalid `[build.wasm-opt] level` value `{}`: expected one of \
                 `0`, `1`, `2`, `3`, `4`, `s`, `z`.",
                self.level
            );
        }
        Ok(())
    }
}

/// The `[memory]` table: the linear memory the emitted module declares and the
/// share of it the shadow stack occupies.
///
/// Both keys are optional and an absent table is identical to a table with
/// neither key — there is no state where declaring `[memory]` means something on
/// its own, unlike `[build.wasm-opt]` whose presence is what enables the
/// optimizer. That is why this is a plain field with a `Default` rather than an
/// `Option`: "the user said nothing" and "the user said nothing in particular"
/// must resolve to the same memory.
///
/// The keys are kept as raw `Option`s rather than eagerly resolved into a
/// [`MemoryLayout`] because which keys were *set* is information the resolved
/// layout no longer carries, and forwarding needs it: an `infs` that forwarded a
/// resolved layout would send `--memory-pages 1 --stack-size 65536` for a project
/// with no `[memory]` table at all, turning every build into a layout request and
/// tripping the compiler-ABI gate for a project that asked for nothing.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    /// Linear memory size in 64 KiB pages. Absent means one page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,

    /// Shadow stack size in bytes. Absent means 64 KiB.
    #[serde(
        rename = "stack-size",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub stack_size: Option<u32>,
}

impl MemoryConfig {
    /// Returns true when the project asked for no particular memory, which is
    /// both the serialization skip condition and the "forward nothing" test.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.pages.is_none() && self.stack_size.is_none()
    }

    /// The declared keys resolved into the shared compiler vocabulary, with every
    /// absent key filled from the default layout.
    ///
    /// The resolution is the same call [`Self::validate`] makes on load, so this
    /// cannot disagree with what the loader accepted.
    ///
    /// # Errors
    ///
    /// Returns the diagnostic rejecting the declared memory, naming the manifest
    /// spelling of the keys. For a manifest that came from
    /// [`InferenceToml::from_toml`] this cannot fail — validation already ran. The
    /// fallible signature is for the other constructor: the fields are public, so
    /// a test or tool that builds a `MemoryConfig` in memory can populate them
    /// without ever passing through the loader, and this is where such a value is
    /// checked.
    pub fn resolved_layout(&self) -> Result<MemoryLayout> {
        MemoryLayout::resolve(self.pages, self.stack_size, MemoryLayoutSource::Manifest)
            .map_err(Into::into)
    }

    /// Validates the declared keys as the layout they complete to.
    ///
    /// # Errors
    ///
    /// Returns an error naming the offending value when the two numbers do not
    /// describe a memory a module can declare.
    fn validate(&self) -> Result<()> {
        self.resolved_layout().map(|_| ())
    }
}

/// Verification configuration for Rocq output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerificationConfig {
    /// Output directory for generated Rocq proofs.
    #[serde(default = "default_output_dir", rename = "output-dir")]
    pub output_dir: String,

    /// Carry a linked library's own universal proof obligations into this
    /// project's proof artifact.
    ///
    /// Off by default: a library's obligations describe the library, and a build
    /// that adopts them is asking to prove them here. Honored only for a build
    /// that resolves to proof mode (`-v`, or `--mode proof`), and withheld with
    /// a note from an explicit `--mode compile -v`: a compile-mode build emits
    /// no verification section for them to join.
    #[serde(default, rename = "adopt-external-specs")]
    pub adopt_external_specs: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            adopt_external_specs: false,
        }
    }
}

impl VerificationConfig {
    /// Returns true if this is the default configuration.
    ///
    /// Every key belongs in this test, not just the first: it is the
    /// `skip_serializing_if` for the whole `[verification]` table, so a key left
    /// out here is a key silently deleted from a manifest on any write path.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.output_dir == default_output_dir() && !self.adopt_external_specs
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

fn default_wasm_opt_enabled() -> bool {
    true
}

fn default_wasm_opt_level() -> String {
    String::from("3")
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
            memory: MemoryConfig::default(),
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
    /// Missing optional sections (`[dependencies]`, `[build]`, `[memory]`,
    /// `[verification]`) are filled in with their defaults; absent fields
    /// within present sections likewise default. Only `[package]` (with at
    /// least `name` and `version`) is required. A key no fixed-schema table
    /// knows is rejected during structural parsing. After that, the `[build]`
    /// and `[memory]` values are validated against their allowed sets.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid TOML, does not match the
    /// manifest schema (`[package]` is missing, or a table carries an unknown
    /// key), or carries an invalid `[build]` or `[memory]` value.
    pub fn from_toml(s: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(s).context("Failed to parse Inference.toml")?;
        manifest.build.validate()?;
        manifest.memory.validate()?;
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
        Self::from_toml(&content).with_context(|| format!("Invalid manifest: {}", path.display()))
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
            wasm_features: Vec::new(),
            wasm_opt: None,
        };
        assert!(!config.is_default());
    }

    #[test]
    fn test_verification_config_is_default() {
        let config = VerificationConfig::default();
        assert!(config.is_default());

        let config = VerificationConfig {
            output_dir: String::from("custom/"),
            ..VerificationConfig::default()
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
    fn wasm_opt_absent_table_parses_as_none() {
        // Backcompat: a manifest without [build.wasm-opt] leaves the field
        // None, and the [build] config still counts as default.
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build]
mode = "compile"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert!(manifest.build.wasm_opt.is_none());
        assert!(manifest.build.is_default());
    }

    #[test]
    fn wasm_opt_bare_table_enables_with_default_level() {
        // Presence alone enables the optimizer; the level defaults to "3".
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        let wasm_opt = manifest.build.wasm_opt.expect("table present");
        assert!(wasm_opt.enabled);
        assert_eq!(wasm_opt.level, "3");
    }

    #[test]
    fn wasm_opt_enabled_false_is_honored() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
enabled = false
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        let wasm_opt = manifest.build.wasm_opt.expect("table present");
        assert!(!wasm_opt.enabled);
        assert_eq!(wasm_opt.level, "3", "level still defaults when disabled");
    }

    #[test]
    fn wasm_opt_auto_install_defaults_to_false() {
        // Absent `auto-install`, a build never reaches out to the network: a
        // missing binary is a hard error, not a silent download.
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        let wasm_opt = manifest.build.wasm_opt.expect("table present");
        assert!(
            !wasm_opt.auto_install,
            "auto-install must default to false (opt-in provisioning)"
        );
        assert!(
            !WasmOptConfig::default().auto_install,
            "the Default impl must also be false"
        );
    }

    #[test]
    fn wasm_opt_auto_install_true_is_parsed() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
auto-install = true
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        let wasm_opt = manifest.build.wasm_opt.expect("table present");
        assert!(
            wasm_opt.auto_install,
            "`auto-install = true` must be honored"
        );
    }

    #[test]
    fn wasm_opt_auto_install_round_trips_through_toml() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
enabled = true
level = "z"
auto-install = true
"#;
        let manifest = InferenceToml::from_toml(src).expect("parses");
        let serialized = manifest.to_toml().expect("serializes");
        let reparsed = InferenceToml::from_toml(&serialized).expect("reparses");
        assert_eq!(manifest, reparsed);
        assert!(
            serialized.contains("auto-install = true"),
            "the auto-install flag must survive serialization under its \
             hyphenated key, got:\n{serialized}"
        );
    }

    #[test]
    fn wasm_opt_accepts_every_documented_level() {
        for level in WASM_OPT_LEVELS {
            let src = format!(
                r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
level = "{level}"
"#
            );
            let manifest = InferenceToml::from_toml(&src)
                .unwrap_or_else(|e| panic!("level `{level}` must be accepted: {e}"));
            assert_eq!(manifest.build.wasm_opt.unwrap().level, *level);
        }
    }

    #[test]
    fn wasm_opt_rejects_unknown_level() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
level = "9"
"#;
        let err = InferenceToml::from_toml(src).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("wasm-opt") && msg.contains("level") && msg.contains('9'),
            "error must name the table, the field, and the offending value, got: {msg}"
        );
    }

    #[test]
    fn wasm_opt_table_round_trips_through_toml() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
enabled = true
level = "z"
"#;
        let manifest = InferenceToml::from_toml(src).expect("parses");
        let serialized = manifest.to_toml().expect("serializes");
        let reparsed = InferenceToml::from_toml(&serialized).expect("reparses");
        assert_eq!(manifest, reparsed);
        assert!(
            serialized.contains("wasm-opt"),
            "the [build.wasm-opt] table must survive serialization, got:\n{serialized}"
        );
    }

    #[test]
    fn wasm_opt_table_makes_build_config_non_default() {
        // A [build.wasm-opt]-only manifest must not round-trip to nothing: the
        // sub-table's presence makes BuildConfig non-default, so the whole
        // [build] table is serialized rather than skipped.
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[build.wasm-opt]
level = "z"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert!(
            !manifest.build.is_default(),
            "a present [build.wasm-opt] table must make BuildConfig non-default"
        );
        let serialized = manifest.to_toml().unwrap();
        assert!(serialized.contains("wasm-opt"));
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
            ..VerificationConfig::default()
        };
        assert_eq!(
            config.normalized_output_dir().unwrap(),
            PathBuf::from("proofs")
        );
    }

    #[test]
    fn normalized_output_dir_accepts_nested_relative() {
        let config = VerificationConfig {
            output_dir: String::from("build/artifacts"),
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
            ..VerificationConfig::default()
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
        InferenceToml::new("scaffolded")
            .write_to_file(&path)
            .unwrap();

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

    /// The adoption key parses, is not the default, and — the part that is easy
    /// to lose — survives a serialization round trip.
    ///
    /// `is_default` is the `skip_serializing_if` for the whole `[verification]`
    /// table, so a key it does not test is a key silently dropped from any
    /// manifest `infs` writes back. The round trip is what makes that a
    /// failure rather than an invisible data loss.
    #[test]
    fn from_toml_parses_and_round_trips_adopt_external_specs() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[verification]
adopt-external-specs = true
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert!(
            manifest.verification.adopt_external_specs,
            "the declared key must reach the parsed manifest"
        );
        assert!(
            !manifest.verification.is_default(),
            "a manifest that asked for adoption is not the default configuration"
        );

        let rendered = manifest.to_toml().unwrap();
        assert!(
            rendered.contains("adopt-external-specs = true"),
            "a written-back manifest must keep the key it was given:\n{rendered}"
        );
        let reparsed = InferenceToml::from_toml(&rendered).unwrap();
        assert_eq!(
            reparsed.verification, manifest.verification,
            "the round trip must preserve the whole table"
        );
    }

    /// The key defaults to off, and an unknown `[verification]` key is still a
    /// hard error: adding a field must not turn the table into a permissive one.
    #[test]
    fn verification_table_defaults_off_and_still_rejects_unknown_keys() {
        let src = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[verification]
output-dir = "proofs/"
"#;
        let manifest = InferenceToml::from_toml(src).unwrap();
        assert!(
            !manifest.verification.adopt_external_specs,
            "an undeclared key must not opt a project into adoption"
        );

        let unknown = r#"
[package]
name = "demo"
version = "0.1.0"
infc_version = "0.1.0"

[verification]
adopt-extenral-specs = true
"#;
        let err = InferenceToml::from_toml(unknown)
            .expect_err("a misspelled key must be a build error, not a silent no-op");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("adopt-extenral-specs"),
            "the rejection must name the offending key, got: {msg}"
        );
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
        for key in [
            "arith",
            "crypto",
            "_priv",
            "a1",
            "crypto::sha256",
            "a::b::c",
        ] {
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
        manifest
            .write_str("[package]\nname = \"x\"\nversion = \"0.1.0\"\n")
            .unwrap();
        let source = temp.child("main.inf");
        source.write_str("").unwrap();

        let found = find_manifest_dir(source.path()).expect("manifest should be found");
        assert_eq!(found, temp.path());
    }

    #[test]
    fn test_find_manifest_dir_walks_up_from_nested_source() {
        let temp = assert_fs::TempDir::new().unwrap();
        let manifest = temp.child(MANIFEST_FILE_NAME);
        manifest
            .write_str("[package]\nname = \"x\"\nversion = \"0.1.0\"\n")
            .unwrap();
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

    /// Builds a manifest whose `[build]` table carries `body`.
    fn manifest_with_build(body: &str) -> String {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n\n\
             [build]\n{body}"
        )
    }

    #[test]
    fn wasm_features_absent_yields_pure_wasm_1_0() {
        let manifest =
            InferenceToml::from_toml(&manifest_with_build("mode = \"compile\"\n")).expect("parses");
        assert!(manifest.build.wasm_features.is_empty());
        assert!(manifest.build.is_default());
        assert_eq!(manifest.build.resolved_wasm_features().unwrap(), Vec::new());
    }

    #[test]
    fn wasm_features_empty_array_is_still_the_default_config() {
        // An explicit `[]` is indistinguishable from absence, so the whole
        // `[build]` table is still skipped on serialization.
        let manifest =
            InferenceToml::from_toml(&manifest_with_build("wasm-features = []\n")).expect("parses");
        assert!(manifest.build.wasm_features.is_empty());
        assert!(manifest.build.is_default());
        let serialized = manifest.to_toml().expect("serializes");
        assert!(
            !serialized.contains("[build]"),
            "a default [build] must be skipped entirely, got:\n{serialized}"
        );
    }

    #[test]
    fn wasm_features_matching_is_case_sensitive() {
        // Pins the same decision `mode` makes: a near-miss is rejected, never
        // silently accepted, because the value selects an instruction set.
        let err = rejection_of(&manifest_with_build("wasm-features = [\"Bulk-Memory\"]\n"));
        assert!(
            err.contains("unknown WebAssembly feature"),
            "`Bulk-Memory` must not resolve, got: {err}"
        );
    }

    #[test]
    fn wasm_features_written_under_the_wasm_opt_header_is_rejected() {
        // The mistake the field-order rule exists to prevent, seen from the user's
        // side: a key placed after the sub-table header belongs to that sub-table
        // in TOML, so it never reaches `[build]`. Before unknown keys were an
        // error this was silently dropped and the artifact shipped at 1.0.
        let err = rejection_of(&manifest_with_build(
            "mode = \"compile\"\n\n[build.wasm-opt]\nlevel = \"z\"\nwasm-features = [\"bulk-memory\"]\n",
        ));
        assert!(
            err.contains("unknown field") && err.contains("wasm-features"),
            "a reparented key must be reported against [build.wasm-opt], got: {err}"
        );
    }

    #[test]
    fn wasm_features_bulk_memory_parses_and_resolves() {
        let manifest =
            InferenceToml::from_toml(&manifest_with_build("wasm-features = [\"bulk-memory\"]\n"))
                .expect("parses");
        assert_eq!(manifest.build.wasm_features, ["bulk-memory"]);
        assert_eq!(
            manifest.build.resolved_wasm_features().unwrap(),
            vec![WasmFeatureName::BulkMemory]
        );
    }

    #[test]
    fn wasm_features_makes_build_config_non_default() {
        // A requested feature must survive a round trip: were the config
        // reported as default, the whole `[build]` table would be skipped and the
        // request would vanish.
        let manifest =
            InferenceToml::from_toml(&manifest_with_build("wasm-features = [\"bulk-memory\"]\n"))
                .expect("parses");
        assert!(!manifest.build.is_default());
        let serialized = manifest.to_toml().expect("serializes");
        assert!(
            serialized.contains("wasm-features = [\"bulk-memory\"]"),
            "the request must survive serialization, got:\n{serialized}"
        );
        assert_eq!(
            InferenceToml::from_toml(&serialized).expect("reparses"),
            manifest
        );
    }

    #[test]
    fn wasm_features_round_trip_keeps_the_key_above_the_wasm_opt_header() {
        // Field declaration order is load-bearing: serialized *after* the
        // `[build.wasm-opt]` header, the array would reparse as one of that
        // sub-table's keys and the request would be lost (or rejected).
        let src = manifest_with_build(
            "mode = \"proof\"\nwasm-features = [\"bulk-memory\"]\n\n\
             [build.wasm-opt]\nlevel = \"z\"\n",
        );
        let manifest = InferenceToml::from_toml(&src).expect("parses");
        let serialized = manifest.to_toml().expect("serializes");

        let key = serialized
            .find("wasm-features")
            .expect("the array must be serialized");
        let header = serialized
            .find("[build.wasm-opt]")
            .expect("the sub-table must be serialized");
        assert!(
            key < header,
            "wasm-features must precede the [build.wasm-opt] header, got:\n{serialized}"
        );
        assert_eq!(
            InferenceToml::from_toml(&serialized).expect("reparses"),
            manifest
        );
    }

    #[test]
    fn wasm_features_rejects_an_unknown_name_listing_the_supported_set() {
        let err = InferenceToml::from_toml(&manifest_with_build("wasm-features = [\"simd\"]\n"))
            .expect_err("`simd` is not in the vocabulary");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown WebAssembly feature") && msg.contains("`bulk-memory`"),
            "the error must name the supported set, got: {msg}"
        );
        assert!(
            msg.contains("`[build] wasm-features`"),
            "the error must name the manifest surface, got: {msg}"
        );
    }

    #[test]
    fn wasm_features_rejects_an_instruction_name_with_the_proposal_to_write() {
        let err =
            InferenceToml::from_toml(&manifest_with_build("wasm-features = [\"memory.fill\"]\n"))
                .expect_err("an instruction is not a feature");
        let msg = err.to_string();
        assert!(
            msg.contains("is an instruction, not a feature") && msg.contains("write `bulk-memory`"),
            "the error must redirect to the proposal name, got: {msg}"
        );
    }

    #[test]
    fn wasm_features_rejects_an_always_on_feature() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "wasm-features = [\"mutable-globals\"]\n",
        ))
        .expect_err("an inherent feature cannot be requested");
        assert!(err.to_string().contains("always enabled"), "got: {err}");
    }

    #[test]
    fn wasm_features_rejects_a_duplicate_entry() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "wasm-features = [\"bulk-memory\", \"bulk-memory\"]\n",
        ))
        .expect_err("a feature may appear at most once");
        assert!(
            err.to_string().contains("listed more than once"),
            "got: {err}"
        );
    }

    #[test]
    fn wasm_features_rejects_surrounding_whitespace_rather_than_trimming() {
        // Trimming would let an invisible typo change the instruction set of a
        // shipped artifact, so the entry is rejected and the message says why.
        let err =
            InferenceToml::from_toml(&manifest_with_build("wasm-features = [\" bulk-memory\"]\n"))
                .expect_err("a padded entry must not resolve");
        assert!(
            err.to_string().contains("surrounding whitespace"),
            "got: {err}"
        );
    }

    // [memory] table ---

    /// Builds a manifest whose `[memory]` table carries `body`.
    fn manifest_with_memory(body: &str) -> String {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n\n\
             [memory]\n{body}"
        )
    }

    #[test]
    fn memory_absent_yields_the_default_layout() {
        let manifest = InferenceToml::from_toml(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n",
        )
        .expect("parses");
        assert!(manifest.memory.is_default());
        assert_eq!(
            manifest.memory.resolved_layout().unwrap(),
            MemoryLayout::default()
        );
    }

    /// An empty `[memory]` table is indistinguishable from no table at all, so
    /// the whole table is skipped on serialize and nothing is forwarded.
    #[test]
    fn memory_table_with_no_keys_is_still_the_default() {
        let manifest = InferenceToml::from_toml(&manifest_with_memory("")).expect("parses");
        assert!(manifest.memory.is_default());
        assert!(
            !manifest.to_toml().unwrap().contains("[memory]"),
            "an empty table must not round-trip into the file"
        );
    }

    /// Each key is independently settable, and the unset one keeps its default.
    /// A project that wants a larger memory has no reason to restate the stack
    /// size, and would silently get a different stack if it restated it wrongly.
    #[test]
    fn each_memory_key_can_be_declared_alone() {
        let pages_only =
            InferenceToml::from_toml(&manifest_with_memory("pages = 4\n")).expect("parses");
        assert_eq!(pages_only.memory.pages, Some(4));
        assert_eq!(pages_only.memory.stack_size, None);
        let layout = pages_only.memory.resolved_layout().unwrap();
        assert_eq!(layout.pages(), 4);
        assert_eq!(layout.stack_size(), MemoryLayout::default().stack_size());

        let stack_only = InferenceToml::from_toml(&manifest_with_memory("stack-size = 32768\n"))
            .expect("parses");
        assert_eq!(stack_only.memory.pages, None);
        assert_eq!(stack_only.memory.stack_size, Some(32_768));
        let layout = stack_only.memory.resolved_layout().unwrap();
        assert_eq!(layout.pages(), MemoryLayout::default().pages());
        assert_eq!(layout.stack_size(), 32_768);
    }

    #[test]
    fn both_memory_keys_parse_and_resolve() {
        let manifest =
            InferenceToml::from_toml(&manifest_with_memory("pages = 2\nstack-size = 32768\n"))
                .expect("parses");
        assert!(!manifest.memory.is_default());
        let layout = manifest.memory.resolved_layout().unwrap();
        assert_eq!(layout.pages(), 2);
        assert_eq!(layout.stack_size(), 32_768);
    }

    /// The stack size is spelled with a hyphen, matching every other multi-word
    /// manifest key. The underscore spelling is a typo, not an alias.
    #[test]
    fn stack_size_is_spelled_with_a_hyphen() {
        let msg = rejection_of(&manifest_with_memory("stack_size = 32768\n"));
        assert!(
            msg.contains("unknown field") && msg.contains("stack_size"),
            "the error must diagnose the underscore spelling, got: {msg}"
        );
        assert!(
            msg.contains("stack-size"),
            "the error must name the key the user meant, got: {msg}"
        );
    }

    #[test]
    fn unknown_key_in_the_memory_table_is_rejected() {
        let msg = rejection_of(&manifest_with_memory("page = 2\n"));
        assert!(
            msg.contains("unknown field") && msg.contains("page"),
            "the error must diagnose an unknown field and name it, got: {msg}"
        );
    }

    /// Validation runs on load, so a manifest that reached a caller has already
    /// been checked — and the diagnostic names the manifest spelling rather than
    /// the compiler flags.
    #[test]
    fn an_unusable_memory_is_rejected_on_load_naming_the_manifest_keys() {
        let msg = rejection_of(&manifest_with_memory("pages = 0\n"));
        assert!(msg.contains("`[memory] pages`"), "got: {msg}");
        assert!(msg.contains("at least one 64 KiB page"), "got: {msg}");

        let msg = rejection_of(&manifest_with_memory("stack-size = 1000\n"));
        assert!(msg.contains("`[memory] stack-size`"), "got: {msg}");
        assert!(
            msg.contains("multiple of the 16-byte frame alignment"),
            "got: {msg}"
        );
    }

    /// A key legal on its own is still judged against the layout it completes to.
    /// Without the fill-then-check order this manifest would load and emit a
    /// stack twice the size of the memory holding it.
    #[test]
    fn a_declared_stack_is_checked_against_the_undeclared_page_count() {
        let msg = rejection_of(&manifest_with_memory("stack-size = 131072\n"));
        assert!(
            msg.contains("does not fit in the linear memory"),
            "got: {msg}"
        );
        assert!(
            InferenceToml::from_toml(&manifest_with_memory("pages = 4\nstack-size = 131072\n"))
                .is_ok(),
            "the same stack loads once the page count makes room for it"
        );
    }

    /// A declared table survives serialization and reparses to an equal manifest,
    /// and its keys stay in `[memory]` rather than reparenting into the sub-table
    /// that precedes them.
    ///
    /// `[build.wasm-opt]` is present deliberately: it is the one sub-table the
    /// manifest emits, and a scalar key written after its header would belong to
    /// `wasm-opt` on the next parse. `[memory]` is safe from that because it is a
    /// table header of its own — an absolute path, not a continuation — but that
    /// is a property of the emitted shape rather than of the field order, so it
    /// is worth pinning rather than assuming.
    #[test]
    fn memory_round_trips_beneath_the_wasm_opt_sub_table() {
        let mut manifest = InferenceToml::new("demo");
        manifest.build.wasm_opt = Some(WasmOptConfig::default());
        manifest.memory = MemoryConfig {
            pages: Some(2),
            stack_size: Some(32_768),
        };

        let serialized = manifest.to_toml().expect("serializes");
        let wasm_opt_at = serialized
            .find("[build.wasm-opt]")
            .expect("the sub-table header must be emitted");
        let memory_at = serialized
            .find("[memory]")
            .expect("the memory table must be emitted");
        assert!(
            wasm_opt_at < memory_at,
            "this test is only meaningful with [memory] written after the sub-table:\n\
             {serialized}"
        );

        let reparsed = InferenceToml::from_toml(&serialized).expect("round-trips");
        assert_eq!(
            reparsed.memory, manifest.memory,
            "the memory keys must survive the round trip rather than reparenting:\n{serialized}"
        );
        assert_eq!(reparsed, manifest);
    }

    /// The rendered cause chain of a rejected manifest.
    ///
    /// A structural parse failure is wrapped in the "Failed to parse
    /// Inference.toml" context, so plain `Display` would show only that wrapper.
    /// The alternate form renders the whole chain, which is what the user sees on
    /// the terminal and what these assertions are about.
    fn rejection_of(src: &str) -> String {
        let err = InferenceToml::from_toml(src)
            .err()
            .unwrap_or_else(|| panic!("this manifest must be rejected:\n{src}"));
        format!("{err:#}")
    }

    #[test]
    fn unknown_key_at_the_manifest_root_is_rejected() {
        let msg = rejection_of(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[buidl]\nmode = \"proof\"\n",
        );
        // `contains("buidl")` alone would be satisfied by toml's source-snippet
        // echo of the offending line, so assert on the diagnosis itself.
        assert!(
            msg.contains("unknown field") && msg.contains("buidl"),
            "the error must diagnose an unknown field and name it, got: {msg}"
        );
        assert!(
            msg.contains("line"),
            "the error must carry the toml span, got: {msg}"
        );
    }

    #[test]
    fn unknown_key_in_every_fixed_schema_table_is_rejected() {
        // The last case is a `[wasm-dependencies]` *entry*: the table's keys are
        // free, the shape of an entry is not.
        let cases = [
            (
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nlicence = \"MIT\"\n",
                "licence",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                 [build]\noptimise = \"release\"\n",
                "optimise",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                 [build.wasm-opt]\nlevels = \"3\"\n",
                "levels",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                 [verification]\noutput_dir = \"p/\"\n",
                "output_dir",
            ),
            (
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                 [wasm-dependencies]\narith = { path = \"a.wasm\", revision = \"1\" }\n",
                "revision",
            ),
        ];
        for (src, offending) in cases {
            let msg = rejection_of(src);
            assert!(
                msg.contains("unknown field") && msg.contains(offending),
                "the error must diagnose an unknown field and name `{offending}`, got: {msg}"
            );
        }
    }

    #[test]
    fn near_miss_wasm_features_spellings_name_the_expected_fields() {
        // The two spellings a user is most likely to reach for. Both must error
        // and list what `[build]` actually accepts, including the real key.
        for typo in ["wasm_features", "wasm-feature"] {
            let msg = rejection_of(&manifest_with_build(&format!(
                "{typo} = [\"bulk-memory\"]\n"
            )));
            assert!(
                msg.contains(typo),
                "the error must name the offending key `{typo}`, got: {msg}"
            );
            assert!(
                msg.contains("wasm-features"),
                "the error must list the expected key, got: {msg}"
            );
        }
    }

    #[test]
    fn wasm_dependencies_still_accepts_arbitrary_keys() {
        // The keys of this table ARE the data (they name dependencies), so it is
        // deliberately not held to a fixed schema.
        let src = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                   [wasm-dependencies]\nanything = { path = \"a.wasm\" }\n\
                   \"crypto::sha256\" = { path = \"b.wasm\" }\n";
        let manifest = InferenceToml::from_toml(src).expect("arbitrary module names must parse");
        assert_eq!(manifest.wasm_dependencies.modules.len(), 2);
    }
}
