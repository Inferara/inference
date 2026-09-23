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
//! [host-imports]          # optional: the host functions the program may bind
//! # Import module -> the fields of it admitted; `use { f } from host::env;`
//! # binds `env.f`. A header with no keys admits no host function at all.
//! env = ["clock_ms"]
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
//! `toml` parser names the offending key and the fields it expected. Three
//! tables accept arbitrary keys, because there the keys *are* the data:
//! `[dependencies]`, whose keys name packages; `[wasm-dependencies]`, whose keys
//! are the logical module names a `use … from <module>;` clause resolves; and
//! `[host-imports]`, whose keys are the import module strings an embedder
//! registers host functions under. Free is not unchecked: the latter two still
//! hold every key to the grammar a key of theirs can take.
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
    HOST_SEGMENT, MemoryLayout, MemoryLayoutSource, TargetName, TargetSource, WasmFeatureName,
    WasmFeatureSource, resolve_target, resolve_wasm_features,
};
use serde::de::{DeserializeSeed, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
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
    "unit",
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

    /// The `[host-imports]` allowlist: the host functions the project's `use …
    /// from host::<module>;` clauses may bind.
    ///
    /// The only table here held as an `Option`, because it is the only one whose
    /// presence is a setting no content can restate. Absent is *no policy*: every
    /// host import the program declares is admitted, as it was before the table
    /// existed. Present and empty — a bare `[host-imports]` header — is a declared
    /// allowlist that admits nothing, which is how a project says it binds no host
    /// function at all. Collapsing the two into one default would turn the
    /// strictest policy a project can write into the most permissive one, silently,
    /// on the next write-back — and an empty table is exactly the shape a TOML
    /// serializer is tempted to drop — so both must survive a
    /// [`Self::to_toml`] / [`Self::from_toml`] round trip, which a test pins.
    ///
    /// Every module's array names at least one field, so the table's key set is
    /// exactly the module set of the `--host-imports` flag it is forwarded as.
    /// That invariant is why an empty array is refused on load rather than read
    /// as "nothing from this module": the flag has no spelling for a module with
    /// no admitted field, so `env = []` would forward no `env` pair, and `infc` —
    /// whose refusal asks for a new `env` key or for a name added to the
    /// existing entry according to whether its flag carries any `env` pair —
    /// would tell this manifest's reader to add a second `env` key, a TOML
    /// duplicate-key error.
    ///
    /// Each field is a plain string today. An entry may later become an untagged
    /// `String | { name = …, … }`, so a producer has somewhere to attach a
    /// contract-spec name, or a name mapping for an import string that is not an
    /// identifier; an `infs` that predates the table form refuses such an entry
    /// loudly rather than misreading it.
    #[serde(
        rename = "host-imports",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub host_imports: Option<HostImports>,

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
/// A well-formed key whose first segment is `host` is refused too. That segment
/// names the embedder in a `use … from` clause, so no linked module can be named
/// under it, and `infc` refuses the `--wasm-dep` such a key would be forwarded
/// as. Refusing it here states the same rule in this manifest's terms, before a
/// compiler is asked to build, and adds the remedy only a project has: the
/// function belongs under `[host-imports]` if the project keeps an allowlist.
/// The rest of the wording follows `infc`'s refusal, shortened, and ends on its
/// closing clause verbatim — a module path whose first segment is `host` is
/// reserved in this position and no longer resolves to a file — because this
/// refusal is now the one that reaches the reader the clause is for: an author
/// whose project built before the reservation existed, who owes a migration
/// rather than having broken a rule.
/// The match is on the first segment and is exact, as `infc`'s is — `hostlib`,
/// `Host` and `a::host` are ordinary module names.
///
/// # Errors
///
/// Returns an error naming the offending key when it is not a well-formed
/// logical name, or when its first segment is reserved for host imports.
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
    if segments[0] == HOST_SEGMENT {
        bail!(
            "invalid [wasm-dependencies] key `{key}`: `{HOST_SEGMENT}` is reserved as the \
             first segment of a `use … from` clause for imports the embedder supplies, so \
             no linked module can be named under it. If this entry was written for a host \
             import, delete it — a `use … from {HOST_SEGMENT}::<module>;` clause needs no \
             dependency entry — and list the function under `[host-imports]` if the \
             project keeps an allowlist. Rename the module — and the `use … from` clause \
             that binds it — to a name outside `{HOST_SEGMENT}` only if you meant a linked \
             `.wasm` module named `{key}`: a module path whose first segment is \
             `{HOST_SEGMENT}` is reserved in this position and no longer resolves to a file."
        );
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

/// The `[host-imports]` table: the host functions a project admits, as one array
/// of field names per import module.
///
/// ```toml
/// [host-imports]
/// fprime_core = ["telemetry", "command"]
/// env = ["clock_ms"]
/// ```
///
/// A key is the WebAssembly import module string an embedder registers — the one
/// segment after `host::` in the `use … from host::<module>;` clause that binds
/// the import — and its value names the fields of that module the program may
/// bind. The keys are the data, as they are in `[wasm-dependencies]`, so the table
/// has no fixed schema.
///
/// A `BTreeMap` rather than the `HashMap` the other data-keyed tables hold, so a
/// manifest written back by [`InferenceToml::to_toml`] lists its modules in one
/// order whatever the hash seed. The order the allowlist is forwarded in does not
/// rest on that: [`Self::admitted_pairs`] sorts its pairs itself.
///
/// Deserialized by hand, and serialized as the bare map it holds. The derived
/// `#[serde(flatten)]` form the other data-keyed tables use buffers the table
/// before reading it, which drops every value's position: measured, an
/// `env = "clock_ms"` written where an array belongs was reported against the
/// `[host-imports]` header with no key named anywhere in the diagnosis. Read as
/// a map, each value keeps its span, and the seed that reads it puts its key
/// into the diagnosis, so a value of the wrong shape is named however it is
/// laid out.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct HostImports {
    /// Map of import module string to the host function names admitted under it.
    pub modules: BTreeMap<String, Vec<String>>,
}

impl<'de> Deserialize<'de> for HostImports {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(HostImportsVisitor)
    }
}

/// Reads `[host-imports]` as a map, one module at a time, so each value is read
/// by a [`FieldNames`] seed that knows its key.
struct HostImportsVisitor;

impl<'de> Visitor<'de> for HostImportsVisitor {
    type Value = HostImports;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a table mapping import module names to arrays of host function names")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<HostImports, A::Error> {
        let mut modules = BTreeMap::new();
        while let Some(module) = map.next_key::<String>()? {
            let fields = map.next_value_seed(FieldNames { module: &module })?;
            modules.insert(module, fields);
        }
        Ok(HostImports { modules })
    }
}

/// One `[host-imports]` value, read as the field names admitted under `module`.
/// The key is carried only so a value of the wrong shape is refused by name.
struct FieldNames<'a> {
    module: &'a str,
}

impl<'de> DeserializeSeed<'de> for FieldNames<'_> {
    type Value = Vec<String>;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Vec<String>, D::Error> {
        let module = self.module;
        Vec::<String>::deserialize(deserializer).map_err(|err| {
            serde::de::Error::custom(format!(
                "invalid [host-imports] entry `{module}`: the value must be an array of \
                 strings, one per host function admitted under `{module}` ({})",
                err.to_string().trim_end()
            ))
        })
    }
}

impl HostImports {
    /// Every admitted `(module, field)` pair, sorted by module and then field.
    ///
    /// Sorted here rather than read in the map's order, so the order a build
    /// forwards and echoes is a property of the policy and not of the manifest
    /// that spelled it: two manifests listing the same functions in different
    /// orders hand `infc` one flag and put one line in a build log.
    #[must_use]
    pub fn admitted_pairs(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> = self
            .modules
            .iter()
            .flat_map(|(module, fields)| {
                fields
                    .iter()
                    .map(move |field| (module.as_str(), field.as_str()))
            })
            .collect();
        pairs.sort_unstable();
        pairs
    }

    /// Refuses a table that cannot be forwarded as the policy it spells.
    ///
    /// Runs on load, from [`InferenceToml::from_toml`], where a
    /// `[wasm-dependencies]` key is checked only on the paths that forward it. The
    /// divergence is about what a malformed entry is. A dependency key that cannot
    /// be forwarded fails the build that needed it; a malformed allowlist is a
    /// defect in a security policy, and a manifest carrying one must fail every
    /// command that loads it, not only the ones that happen to forward it.
    ///
    /// Checked module by module in key order, and within a module in the order
    /// written: the key is one identifier segment, the array names at least one
    /// field (the invariant [`InferenceToml::host_imports`] documents), each field
    /// is an identifier, and no field is listed twice.
    ///
    /// # Errors
    ///
    /// Returns a refusal naming the offending key, and the offending field where
    /// there is one.
    fn validate(&self) -> Result<()> {
        for (module, fields) in &self.modules {
            validate_host_import_module(module)?;
            if fields.is_empty() {
                bail!(
                    "invalid [host-imports] entry `{module} = []`: an empty array admits no \
                     host function under `{module}`, which is almost certainly a mistake. \
                     Delete the key, or list the fields. A project that binds no host \
                     function at all says so with the table alone: a `[host-imports]` \
                     header with no keys under it."
                );
            }
            let mut listed = BTreeSet::new();
            for field in fields {
                if !is_logical_name_segment(field) {
                    let correction = field_without_its_module(module, field)
                        .map_or_else(String::new, |bare| {
                            format!(" Write `\"{bare}\"`: the module is already the key.")
                        });
                    bail!(
                        "invalid [host-imports] entry `{module}`: `{field:?}` is not a host \
                         function name. A field is a name the `use {{ … }} from \
                         {HOST_SEGMENT}::{module};` clause binds, so it is an ASCII \
                         identifier — a letter or `_`, then letters, digits or \
                         `_`.{correction}"
                    );
                }
                if !listed.insert(field.as_str()) {
                    bail!(
                        "invalid [host-imports] entry `{module}`: `{field:?}` is listed \
                         twice; list each host function once."
                    );
                }
            }
        }
        Ok(())
    }
}

/// The bare field name in `field` when it is a whole pair spelled under `module`,
/// the entry's own key, or `None` when it is anything else.
///
/// Every surface near the table spells a pair with its module attached: the
/// echo, the `host imports:` inventory line and the `--host-imports` token as
/// `env.clock_ms`, the clause as `host::env` and `clock_ms` together. Copying one
/// of those into the array is the likeliest way to write a field that is not an
/// identifier, and the one whose repair can be read off the entry — the part
/// after the key. Both separators are accepted, and a leading `host::` too, but
/// only under the entry's own key: a pair naming another module may belong under
/// another key, and naming a spelling for it would be a guess.
fn field_without_its_module<'a>(module: &str, field: &'a str) -> Option<&'a str> {
    let after_module = |spelled: &'a str| {
        let rest = spelled.strip_prefix(module)?;
        let bare = rest.strip_prefix('.').or_else(|| rest.strip_prefix("::"))?;
        is_logical_name_segment(bare).then_some(bare)
    };
    after_module(field).or_else(|| {
        field
            .strip_prefix(HOST_SEGMENT)
            .and_then(|rest| rest.strip_prefix("::"))
            .and_then(after_module)
    })
}

/// Refuses a `[host-imports]` key that is not one import module name.
///
/// A key containing `::` gets a sentence of its own, because it is the
/// transcription mistake the clause invites: `use { clock_ms } from host::env;`
/// reads as a path, and both `host::env` and `env::v2` look like keys a path
/// could have. The one right spelling is named where it can be read out of the
/// key — a leading `host::` stripped from a single segment — and otherwise the
/// clause-to-key mapping is shown by example rather than guessed.
fn validate_host_import_module(module: &str) -> Result<()> {
    if module.is_empty() {
        bail!("invalid [host-imports] key: the module name is empty");
    }
    if module.contains("::") {
        let unprefixed = module
            .strip_prefix(HOST_SEGMENT)
            .and_then(|rest| rest.strip_prefix("::"))
            .filter(|rest| is_logical_name_segment(rest));
        let correction = unprefixed.map_or_else(
            || {
                format!(
                    "For example, the module of `use {{ clock_ms }} from {HOST_SEGMENT}::env;` \
                     is listed as `env`."
                )
            },
            |rest| format!("Write `{rest}`."),
        );
        bail!(
            "invalid [host-imports] key `{module}`: a key is the WebAssembly import module \
             string an embedder registers, and that string is flat — one segment, the one \
             after `{HOST_SEGMENT}::` in the `use … from {HOST_SEGMENT}::<module>;` clause — \
             so it never contains `::`, and the `{HOST_SEGMENT}::` prefix of the clause names \
             the provider and is not part of it. {correction}"
        );
    }
    if !is_logical_name_segment(module) {
        bail!(
            "invalid [host-imports] key `{module}`: a key is the one segment after \
             `{HOST_SEGMENT}::` in the `use … from {HOST_SEGMENT}::<module>;` clause that \
             binds the import, so it is an ASCII identifier — a letter or `_`, then letters, \
             digits or `_`."
        );
    }
    Ok(())
}

/// Build configuration section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    /// The runtime the emitted module is built for.
    ///
    /// The axis is the runtime, not a compiler back end and not a target triple:
    /// `"wasm32"` — the default — is the generic value, a module for any
    /// WebAssembly embedder that imposes no ABI of its own.
    ///
    /// A validated `String` for the same reason [`mode`](Self::mode) is: the
    /// vocabulary is already modelled by
    /// `inference_compiler_interface::TargetName`, and a serde enum here would be
    /// a second spelling of it. Resolved against that vocabulary by
    /// [`Self::resolved_target`]; validation runs on load, so a manifest that
    /// reached a caller has already been checked, and matching is exact and
    /// case-sensitive with no whitespace trimmed.
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

    /// The `target` field resolved into the shared compiler vocabulary.
    ///
    /// Callers get a typed target without knowing how the raw string is spelled
    /// or validated. The resolution is the same call [`Self::validate`] makes on
    /// load, so this cannot disagree with what the loader accepted.
    ///
    /// # Errors
    ///
    /// Returns the diagnostic rejecting a name that is not a supported target.
    /// For a manifest that came from [`InferenceToml::from_toml`] this cannot
    /// fail — validation already ran. The fallible signature is for the other
    /// constructor: [`Self::target`] is a public `String` field, so a test or
    /// tool that builds a `BuildConfig` in memory can set it without ever passing
    /// through the loader, and this is where such a value is checked.
    pub fn resolved_target(&self) -> Result<TargetName> {
        resolve_target(&self.target, TargetSource::Manifest)
            .map_err(|message| anyhow::anyhow!("{message}"))
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

    /// Validates the `target` field against the shared vocabulary, then the
    /// `mode` field, accepting only `"compile"` or `"proof"` (case-sensitive —
    /// TOML config values are conventionally lowercase, and matching the exact
    /// `infc --mode` flag spelling avoids surprising near-misses like
    /// `"Proof"`), then the `wasm-features` entries and the `[build.wasm-opt]`
    /// sub-table.
    ///
    /// The checking order is fixed — `target`, then `mode`, then
    /// `wasm-features`, then the `[build.wasm-opt]` sub-table, then the pairings
    /// two individually-valid keys are not allowed to form — and is independent
    /// of the order the keys appear in the file: TOML leaves that order free and
    /// nothing here can see it. A manifest with more than one mistake therefore
    /// reports whichever field comes first in *this* order, not the first mistake
    /// written. That is a contract, pinned by a test, so a caller may rely on
    /// which of two errors it sees. The pairing check comes last so a manifest
    /// whose optimizer table is itself malformed is told that first, rather than
    /// being told to delete a table it would have to fix anyway.
    ///
    /// [`optimize`](Self::optimize) is deliberately not checked: the value is
    /// recorded and not yet consumed, so there is no behavior for a wrong one to
    /// change and nothing to name in a message.
    ///
    /// # Errors
    ///
    /// Returns an error naming the field and the allowed values when `target` is
    /// not a supported target, when `mode` is neither `"compile"` nor `"proof"`,
    /// when a `wasm-features` entry is not a supported proposal name, when
    /// `[build.wasm-opt]` is invalid, or when the resolved target refuses the
    /// mode, a requested feature, or the optimizer.
    fn validate(&self) -> Result<()> {
        let target = self.resolved_target()?;
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
        self.validate_target_pairings(target)
    }

    /// Refuses a `[build] target` paired with another key the target cannot
    /// honor: `mode = "proof"`, a non-empty `wasm-features` list, or a
    /// `[build.wasm-opt]` table.
    ///
    /// The first two arms ask [`TargetName`] rather than naming a target, so the
    /// fact of what a target narrows is stated once, in the vocabulary both
    /// surfaces share, and a name added there arrives here already answered. The
    /// optimizer arm names the Stellar target because no predicate models it:
    /// what it refuses is not a property of the emitted instruction set but of
    /// whichever external Binaryen the machine happens to have.
    ///
    /// Why refuse a pairing the compiler would refuse anyway: without these the
    /// build spawns `infc`, which fails on the emission-side mirror of the same
    /// fact — in a message that names neither the manifest nor the key to edit,
    /// because at that point the request has become a flag. A key the user wrote
    /// is a key the diagnostic can point at.
    ///
    /// Each refusal in turn:
    ///
    /// - **Proof mode.** Proof-mode output carries Inference's custom `0xfc`
    ///   non-deterministic instructions, which nothing outside Inference's own
    ///   tooling decodes; a target that refuses them refuses the mode.
    /// - **`wasm-features`.** A target that permits no post-MVP proposal permits
    ///   no entry in the list, and honoring one would emit an instruction family
    ///   the target's rule set excludes. The predicate is per-proposal and the
    ///   vocabulary holds exactly one proposal, so a single question answers the
    ///   whole list; a second proposal turns this into a per-entry question.
    /// - **`[build.wasm-opt]`.** A Stellar artifact carries things nothing else
    ///   in the toolchain produces, two of them the layer a host decodes: the
    ///   value-ABI wrappers every exported method is reached through, and the
    ///   `contractenvmetav0` custom section a host refuses to upload a module
    ///   without. Whether an external Binaryen preserves a custom section, and
    ///   what it does to the wrappers, is a property of whichever `wasm-opt` the
    ///   machine happens to have — the version is not pinned by anything here —
    ///   and a module that lost either is refused at upload or, worse, uploads
    ///   and misdecodes its arguments. A build-time refusal costs a manifest
    ///   edit; the alternative costs a deployment. Both of those are things one
    ///   target's artifact carries and no other's does, which is what the name
    ///   equality says. A target whose module is the default's bytes carries
    ///   neither, so it keeps the optimizer — and wants it most, since the
    ///   runtimes that narrow a build this way are the ones with the least room
    ///   to load it into.
    ///
    /// The arms run in the order the per-key checks in [`Self::validate`] run —
    /// `mode`, then `wasm-features`, then the optimizer sub-table — so a manifest
    /// wrong in two ways is told about the same key either kind of check would
    /// have reported first.
    ///
    /// A load-time error rather than a build-time one so that every command
    /// reports the same key. Deciding the mode arm once the *effective* mode is
    /// known would have each command answer differently on one unchanged
    /// manifest: `infs run` forces compile mode and would proceed, `infs build
    /// --mode compile` would proceed, a plain `infs build` would not — and the
    /// user would learn which of their commands the manifest is wrong for rather
    /// than that it is wrong.
    ///
    /// The cost is real and worth naming: `infs build --mode compile` on a
    /// `mode = "proof"` manifest whose target has no proof mode asks for a build
    /// the toolchain can make, and this refuses it until `[build] mode` is
    /// edited.
    fn validate_target_pairings(&self, target: TargetName) -> Result<()> {
        if self.mode == "proof" && !target.supports_proof_mode() {
            let remediation = target
                .proof_refusal_remediation()
                .map_or_else(String::new, |clause| format!(" {clause}"));
            bail!(
                "`[build] target = \"{}\"` cannot be combined with \
                 `[build] mode = \"proof\"`. Proof mode emits Inference's \
                 custom non-deterministic instructions (the `0xfc` family), \
                 which no runtime outside Inference's own tooling decodes, so \
                 the artifact this manifest describes is one the runtime it \
                 names could not load. Set `mode = \"compile\"`, or build for \
                 a target that supports proof mode.{remediation}",
                target.as_str()
            );
        }
        if !self.wasm_features.is_empty() && !target.permits_bulk_memory() {
            bail!(
                "`[build] target = \"{}\"` cannot be combined with a \
                 non-empty `[build] wasm-features` list. This target narrows \
                 what a build may contain to the WebAssembly 1.0 instruction \
                 set, so no post-MVP proposal may be requested for it. Remove \
                 the `wasm-features` entries, or build for a target that \
                 permits them.",
                target.as_str()
            );
        }
        if target == TargetName::Stellar && self.wasm_opt.is_some() {
            bail!(
                "`[build] target = \"{}\"` cannot be combined with a \
                 `[build.wasm-opt]` table. A Stellar contract's value-ABI \
                 wrappers and its `contractenvmetav0` metadata section are the \
                 layer a host is trusted to decode, and whether an external \
                 `wasm-opt` preserves them depends on the Binaryen version \
                 installed, which nothing here pins. Remove the \
                 `[build.wasm-opt]` table, or build for a target that permits \
                 it.",
                TargetName::Stellar.as_str()
            );
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

/// Derived from the shared vocabulary rather than spelled here, so the manifest
/// default and the `infc --target` default cannot drift into two answers.
fn default_target() -> String {
    TargetName::DEFAULT.as_str().to_string()
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
            host_imports: None,
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
    /// logical module name, or its first segment is the reserved `host`.
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
    /// within present sections likewise default. `[host-imports]` is the one
    /// exception, and stays absent: see [`Self::host_imports`]. Only `[package]`
    /// (with at least `name` and `version`) is required. A key no fixed-schema
    /// table knows is rejected during structural parsing. After that, the
    /// `[build]` and `[memory]` values are validated against their allowed sets,
    /// and a `[host-imports]` table against the rules its forwarding needs.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid TOML, does not match the
    /// manifest schema (`[package]` is missing, or a table carries an unknown
    /// key), or carries an invalid `[build]`, `[memory]` or `[host-imports]`
    /// value.
    pub fn from_toml(s: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(s).context("Failed to parse Inference.toml")?;
        manifest.build.validate()?;
        manifest.memory.validate()?;
        if let Some(host_imports) = &manifest.host_imports {
            host_imports.validate()?;
        }
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

    /// `is_default` is the round-trip gate: a config it reports as default is
    /// dropped from the serialized manifest entirely, so every field that can
    /// hold a value other than the default has to make it answer `false`.
    ///
    /// The values are built in memory on purpose, and that is all this test
    /// claims. It is not evidence that a manifest carrying them loads —
    /// `target = "wasm64"` in particular is refused by [`BuildConfig::validate`]
    /// and never reaches a round trip, which
    /// `a_target_outside_the_vocabulary_fails_to_load` is what pins. What is
    /// covered here is the totality of `is_default` over the public fields, which
    /// a caller can still set without ever going through the loader.
    #[test]
    fn test_build_config_is_default() {
        assert!(BuildConfig::default().is_default());

        let non_defaults = [
            BuildConfig {
                target: String::from("wasm64"),
                ..BuildConfig::default()
            },
            BuildConfig {
                optimize: String::from("release"),
                ..BuildConfig::default()
            },
            BuildConfig {
                mode: String::from("proof"),
                ..BuildConfig::default()
            },
            BuildConfig {
                wasm_features: vec![String::from("bulk-memory")],
                ..BuildConfig::default()
            },
            BuildConfig {
                wasm_opt: Some(WasmOptConfig::default()),
                ..BuildConfig::default()
            },
        ];
        for config in non_defaults {
            assert!(
                !config.is_default(),
                "a non-default field must be reported: {config:?}"
            );
        }
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
        for &word in &["fn", "let", "struct", "type", "return", "if", "else", "unit"] {
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

    // `[build] target` ---

    #[test]
    fn the_default_target_loads_and_survives_a_round_trip() {
        let manifest = InferenceToml::from_toml(&manifest_with_build(
            "target = \"wasm32\"\nmode = \"proof\"\n",
        ))
        .expect("`wasm32` is the supported target");
        assert_eq!(manifest.build.target, "wasm32");
        assert_eq!(
            manifest.build.resolved_target().expect("resolves"),
            TargetName::Wasm32
        );

        let serialized = manifest.to_toml().expect("serializes");
        assert_eq!(
            InferenceToml::from_toml(&serialized).expect("reparses"),
            manifest
        );
    }

    /// The key used to be accepted whatever it said, so a project could declare a
    /// target that did not exist and get a default build with no sign of it. It
    /// is now validated on load, against the same vocabulary `infc --target`
    /// uses.
    #[test]
    fn a_target_outside_the_vocabulary_fails_to_load() {
        let err = InferenceToml::from_toml(&manifest_with_build("target = \"wasm64\"\n"))
            .expect_err("`wasm64` names no target");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown compilation target") && msg.contains("`wasm32`"),
            "the error must name the supported set, got: {msg}"
        );
        assert!(
            msg.contains("`[build] target`"),
            "the error must name the manifest surface, got: {msg}"
        );
    }

    /// A former spelling of a target that is now requestable earns the sentence
    /// redirecting to the current one, on the manifest surface as much as on the
    /// flag.
    #[test]
    fn a_reserved_target_fails_to_load_as_one() {
        for reserved in inference_compiler_interface::RESERVED_TARGET_NAMES {
            let err =
                InferenceToml::from_toml(&manifest_with_build(&format!("target = \"{reserved}\"\n")))
                    .expect_err("a reserved name is not requestable");
            let msg = err.to_string();
            assert!(
                msg.contains("is the former name of the `stellar` target"),
                "`{reserved}` must be refused as reserved, got: {msg}"
            );
        }
    }

    /// Every requestable target loads from the manifest surface, which is what
    /// makes the reserved-name test above non-vacuous: the two slices are
    /// disjoint and this one is the half that must succeed.
    #[test]
    fn every_requestable_target_loads_from_the_manifest() {
        for target in TargetName::ALL {
            let manifest = InferenceToml::from_toml(&manifest_with_build(&format!(
                "target = \"{}\"\n",
                target.as_str()
            )))
            .unwrap_or_else(|err| panic!("`{}` must load: {err}", target.as_str()));
            assert_eq!(
                manifest.build.resolved_target().expect("resolves"),
                target,
                "the loaded manifest must resolve to the target it named"
            );
        }
    }

    /// The optimizer table is refused *for the Stellar target only*, so both
    /// halves are asserted: the pairing fails, and each key on its own still
    /// loads. Without the second half this would pass just as well if
    /// `[build.wasm-opt]` had been broken outright.
    #[test]
    fn the_stellar_target_refuses_the_optimizer_table() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\n\n[build.wasm-opt]\nlevel = \"z\"\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`[build] target = \"stellar\"`") && msg.contains("`[build.wasm-opt]`"),
            "the refusal must name both keys, got: {msg}"
        );

        InferenceToml::from_toml(&manifest_with_build("target = \"stellar\"\n"))
            .expect("the Stellar target alone loads");
        InferenceToml::from_toml(&manifest_with_build(
            "[build.wasm-opt]\nlevel = \"z\"\n",
        ))
        .expect("the optimizer table alone loads");
    }

    /// Proof mode is refused *for the Stellar target only*, and both halves are
    /// asserted: the pairing fails, and each key on its own still loads. Without
    /// the second half this would pass just as well if `mode = "proof"` had been
    /// broken outright.
    ///
    /// Left to `infc`, this manifest dies on the emission-side mirror of the same
    /// rule, in a message naming a `--mode` flag the user never typed rather than
    /// the manifest key they did.
    #[test]
    fn the_stellar_target_refuses_proof_mode() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\nmode = \"proof\"\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`[build] target = \"stellar\"`") && msg.contains("`[build] mode"),
            "the refusal must name both keys, got: {msg}"
        );

        InferenceToml::from_toml(&manifest_with_build("target = \"stellar\"\n"))
            .expect("the Stellar target alone loads");
        InferenceToml::from_toml(&manifest_with_build("mode = \"proof\"\n"))
            .expect("proof mode alone loads");
    }

    /// A `wasm-features` request is refused for the Stellar target, with the same
    /// both-halves assertion: an empty list is not what is being refused, a
    /// non-empty one is.
    #[test]
    fn the_stellar_target_refuses_a_wasm_features_request() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\nwasm-features = [\"bulk-memory\"]\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`[build] target = \"stellar\"`")
                && msg.contains("`[build] wasm-features`"),
            "the refusal must name both keys, got: {msg}"
        );

        InferenceToml::from_toml(&manifest_with_build("target = \"stellar\"\n"))
            .expect("the Stellar target alone loads");
        InferenceToml::from_toml(&manifest_with_build("wasm-features = [\"bulk-memory\"]\n"))
            .expect("the feature request alone loads");

        InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\nwasm-features = []\n",
        ))
        .expect("an empty list asks for nothing and is not a pairing");
    }

    /// Proof mode is refused for the `SpaceWasm` target too — the mode arm asks
    /// the vocabulary rather than naming a target, so a name arrives here
    /// already answered — and the refusal carries the remediation only this
    /// target has: its compile-mode bytes are the `wasm32` build's, so the
    /// proof is one command away rather than unavailable.
    ///
    /// Fails if the remediation clause is dropped from
    /// `TargetName::proof_refusal_remediation`, or if the message is assembled
    /// without it.
    #[test]
    fn the_spacewasm_target_refuses_proof_mode_naming_the_build_that_proves_it() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"spacewasm\"\nmode = \"proof\"\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`[build] target = \"spacewasm\"`") && msg.contains("`[build] mode"),
            "the refusal must name both keys, got: {msg}"
        );
        assert!(
            msg.contains("Prove the program in a `wasm32` build"),
            "the refusal must name the build that does produce a proof, got: {msg}"
        );

        InferenceToml::from_toml(&manifest_with_build("target = \"spacewasm\"\n"))
            .expect("the SpaceWasm target alone loads");
    }

    /// Stellar's refusal must not acquire `SpaceWasm`'s remediation: its artifact
    /// is a rewrite of the `wasm32` module rather than the same bytes, so
    /// "prove at `wasm32`, deploy this build" is not a sentence that is true
    /// there without the qualification the book gives it.
    ///
    /// Fails if the per-name clause is replaced by one sentence for every
    /// non-proving target.
    #[test]
    fn the_stellar_proof_refusal_carries_no_per_target_remediation() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\nmode = \"proof\"\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.ends_with("or build for a target that supports proof mode."),
            "the generic remediation must be the whole of it, got: {msg}"
        );
    }

    /// A `wasm-features` request is refused for the `SpaceWasm` target, with the
    /// same both-halves assertion the Stellar row uses.
    ///
    /// Fails if `SpaceWasm` starts permitting bulk memory, or if the feature arm
    /// starts naming a target instead of asking the predicate.
    #[test]
    fn the_spacewasm_target_refuses_a_wasm_features_request() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"spacewasm\"\nwasm-features = [\"bulk-memory\"]\n",
        ))
        .expect_err("the pairing is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("`[build] target = \"spacewasm\"`")
                && msg.contains("`[build] wasm-features`"),
            "the refusal must name both keys, got: {msg}"
        );

        InferenceToml::from_toml(&manifest_with_build(
            "target = \"spacewasm\"\nwasm-features = []\n",
        ))
        .expect("an empty list asks for nothing and is not a pairing");
    }

    /// The optimizer table *loads* for the `SpaceWasm` target, and the asymmetry
    /// with Stellar is deliberate rather than inherited from a name equality
    /// nobody revisited: what that arm protects is a contract's value-ABI
    /// wrappers and its `contractenvmetav0` section, and a target whose module
    /// is the default's bytes has neither. A size-constrained runtime is the one that
    /// wants the optimizer most.
    ///
    /// Fails the moment the optimizer arm is generalized from the one target to
    /// "any non-default target" — which is the shape a reader is most likely to
    /// mistake it for.
    #[test]
    fn the_spacewasm_target_keeps_the_optimizer_table() {
        let manifest = InferenceToml::from_toml(&manifest_with_build(
            "target = \"spacewasm\"\n\n[build.wasm-opt]\nlevel = \"s\"\n",
        ))
        .expect("the SpaceWasm target permits the optimizer");
        assert_eq!(
            manifest.build.resolved_target().expect("resolves"),
            TargetName::SpaceWasm
        );
        assert!(
            manifest.build.wasm_opt.is_some(),
            "the optimizer table must survive the load, not merely fail to refuse it"
        );
    }

    /// The pairing check runs after the per-key checks, so a manifest that is
    /// wrong in both ways is told about the malformed table rather than about a
    /// pairing it would still have to fix afterwards.
    #[test]
    fn a_malformed_optimizer_table_outranks_the_pairing_refusal() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "target = \"stellar\"\n\n[build.wasm-opt]\nlevel = \"nope\"\n",
        ))
        .expect_err("the malformed level is refused");
        let msg = err.to_string();
        assert!(
            msg.contains("level"),
            "the malformed level must be reported first, got: {msg}"
        );
    }

    /// Whitespace is rejected, never trimmed, and the message names it — inside a
    /// TOML string a trailing space is invisible in the echoed value.
    #[test]
    fn target_matching_is_exact_on_the_manifest_surface() {
        for near_miss in ["Wasm32", "wasm32 ", " wasm32", "SpaceWasm", "spacewasm "] {
            let err = InferenceToml::from_toml(&manifest_with_build(&format!(
                "target = \"{near_miss}\"\n"
            )))
            .expect_err("near-misses do not resolve");
            assert!(
                err.to_string().contains("unknown compilation target"),
                "`{near_miss}` must be refused, got: {err}"
            );
        }

        let err = InferenceToml::from_toml(&manifest_with_build("target = \"wasm32 \"\n"))
            .expect_err("whitespace is rejected");
        assert!(
            err.to_string().contains("surrounding whitespace"),
            "the space must be named as the cause, got: {err}"
        );
    }

    /// The documented checking order is a contract, and the manifest that could
    /// falsify it is one whose keys are written in the opposite order: TOML fixes
    /// no key order and validation never looks at the source text, so the
    /// `target` error must surface even though the `mode` mistake is written
    /// first.
    #[test]
    fn two_mistakes_report_the_field_checked_first() {
        let err = InferenceToml::from_toml(&manifest_with_build(
            "mode = \"release\"\ntarget = \"wasm64\"\n",
        ))
        .expect_err("both values are invalid");
        let msg = err.to_string();
        assert!(
            msg.contains("unknown compilation target"),
            "`target` is checked before `mode`, got: {msg}"
        );
        assert!(
            !msg.contains("`[build] mode`"),
            "only the first failing field is reported, got: {msg}"
        );
    }

    #[test]
    fn target_round_trip_keeps_the_key_above_the_wasm_opt_header() {
        // Field declaration order is load-bearing: serialized *after* the
        // `[build.wasm-opt]` header, the key would reparse as one of that
        // sub-table's keys and the manifest would be rejected for an unknown key.
        let src = manifest_with_build(
            "target = \"wasm32\"\nmode = \"proof\"\n\n[build.wasm-opt]\nlevel = \"z\"\n",
        );
        let manifest = InferenceToml::from_toml(&src).expect("parses");
        let serialized = manifest.to_toml().expect("serializes");

        let key = serialized
            .find("target =")
            .expect("the key must be serialized");
        let header = serialized
            .find("[build.wasm-opt]")
            .expect("the sub-table must be serialized");
        assert!(
            key < header,
            "target must precede the [build.wasm-opt] header, got:\n{serialized}"
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

    // `[host-imports]` ---

    fn manifest_with_host_imports(body: &str) -> String {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\ninfc_version = \"0.1.0\"\n\n\
             [host-imports]\n{body}"
        )
    }

    /// A manifest with no `[host-imports]` table holds no policy, and writing it
    /// back must not invent one — not even an empty table, which would be the
    /// strictest policy there is.
    #[test]
    fn an_absent_host_imports_table_stays_absent_through_a_round_trip() {
        let manifest = InferenceToml::new("demo");
        assert_eq!(manifest.host_imports, None, "a new project holds no policy");

        let serialized = manifest.to_toml().expect("serializes");
        assert!(
            !serialized.contains("host-imports"),
            "no policy must write no table:\n{serialized}"
        );
        let reparsed = InferenceToml::from_toml(&serialized).expect("round-trips");
        assert_eq!(reparsed.host_imports, None);
    }

    /// A declared-empty table is the policy that admits nothing, and it must
    /// survive a write-back rather than collapsing into no policy.
    ///
    /// This is the measurement the field exists for: a serializer that drops an
    /// empty table would turn "forbid every host import" into "allow every host
    /// import" on the first `to_toml`, with nothing in the manifest to show it.
    /// The header is asserted as its own line, not merely as a substring, so an
    /// inline `host-imports = {}` rendering — which would also reparse — is told
    /// apart from the header form a reader of the manifest expects.
    #[test]
    fn a_declared_empty_host_imports_table_survives_a_round_trip() {
        let parsed = InferenceToml::from_toml(&manifest_with_host_imports(""))
            .expect("a bare `[host-imports]` header is a valid policy");
        assert_eq!(
            parsed.host_imports,
            Some(HostImports::default()),
            "a bare header is a declared, empty allowlist, not an absent one"
        );

        let serialized = parsed.to_toml().expect("serializes");
        assert!(
            serialized.lines().any(|line| line == "[host-imports]"),
            "the empty table's header must be written back:\n{serialized}"
        );
        let reparsed = InferenceToml::from_toml(&serialized).expect("round-trips");
        assert_eq!(
            reparsed.host_imports,
            Some(HostImports::default()),
            "the empty policy must survive the round trip:\n{serialized}"
        );
    }

    /// A populated table round-trips to an equal manifest whatever order the
    /// modules were written in, and keeps its keys in `[host-imports]` rather than
    /// reparenting them into a sub-table written before it.
    ///
    /// `[wasm-dependencies]` is the table serialized ahead of `[host-imports]`,
    /// and a dependency is written as a sub-table of it, so the fixture declares
    /// one. The header order is asserted before the round trip, because the
    /// reparenting half means nothing unless that sub-table really is written
    /// first.
    #[test]
    fn a_populated_host_imports_table_round_trips() {
        let mut manifest = InferenceToml::from_toml(&manifest_with_host_imports(
            "fprime_core = [\"telemetry\", \"command\"]\nenv = [\"clock_ms\"]\n",
        ))
        .expect("parses");
        manifest.wasm_dependencies.modules.insert(
            "arith".to_string(),
            WasmDependency {
                path: "libs/arith.wasm".to_string(),
            },
        );

        let serialized = manifest.to_toml().expect("serializes");
        let dependency_at = serialized
            .find("[wasm-dependencies.arith]")
            .expect("the dependency must be written as a sub-table header");
        let host_imports_at = serialized
            .find("[host-imports]")
            .expect("the host-imports table must be emitted");
        assert!(
            dependency_at < host_imports_at,
            "this test is only meaningful with [host-imports] written after the sub-table:\n\
             {serialized}"
        );
        let reparsed = InferenceToml::from_toml(&serialized).expect("round-trips");
        assert_eq!(
            reparsed, manifest,
            "the round trip must be lossless:\n{serialized}"
        );

        let reordered = InferenceToml::from_toml(&manifest_with_host_imports(
            "env = [\"clock_ms\"]\nfprime_core = [\"telemetry\", \"command\"]\n",
        ))
        .expect("parses");
        assert_eq!(
            reordered.host_imports, manifest.host_imports,
            "the order modules are written in is not part of the policy"
        );
    }

    /// The pairs come out sorted by module and then by field, whatever order the
    /// manifest listed them in — the order every forward and echo is spelled in.
    #[test]
    fn admitted_pairs_are_sorted_by_module_then_field() {
        let manifest = InferenceToml::from_toml(&manifest_with_host_imports(
            "fprime_core = [\"telemetry\", \"command\"]\nenv = [\"sleep_ms\", \"clock_ms\"]\n",
        ))
        .expect("parses");
        let host_imports = manifest.host_imports.expect("the table is declared");
        assert_eq!(
            host_imports.admitted_pairs(),
            [
                ("env", "clock_ms"),
                ("env", "sleep_ms"),
                ("fprime_core", "command"),
                ("fprime_core", "telemetry"),
            ]
        );
        assert!(
            HostImports::default().admitted_pairs().is_empty(),
            "the empty policy admits no pair"
        );
    }

    /// The keys of `[host-imports]` are the data — any import module name is a
    /// key — while a misspelling of the table's own name at the root is still an
    /// unknown field naming the spelling that exists.
    #[test]
    fn host_imports_keys_are_the_data_but_the_table_name_is_not() {
        let manifest = InferenceToml::from_toml(&manifest_with_host_imports(
            "anything_at_all = [\"f\"]\n_x9 = [\"g\"]\nhost = [\"h\"]\n",
        ))
        .expect("any identifier is an import module name, `host` included");
        assert_eq!(
            manifest.host_imports.expect("declared").modules.len(),
            3,
            "every key is a module"
        );

        for typo in ["host_imports", "host-import", "hostimports"] {
            let msg = rejection_of(&format!(
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n\
                 [{typo}]\nenv = [\"clock_ms\"]\n"
            ));
            assert!(
                msg.contains("unknown field") && msg.contains(&format!("`{typo}`")),
                "a misspelled root table is an unknown field naming it, got: {msg}"
            );
            assert!(
                msg.contains("`host-imports`"),
                "the error must list the spelling that exists, got: {msg}"
            );
        }
    }

    /// A key written as a path is refused with the reason — an import module
    /// string is flat — and with the one right spelling wherever the key holds
    /// it, so `host::env` is told `env`, while `env::v2`, which holds no single
    /// right answer, is shown the clause-to-key mapping by example.
    #[test]
    fn a_path_shaped_host_imports_key_is_refused_as_flat() {
        let msg = rejection_of(&manifest_with_host_imports(
            "\"host::env\" = [\"clock_ms\"]\n",
        ));
        for fragment in [
            "invalid [host-imports] key `host::env`",
            "that string is flat — one segment",
            "so it never contains `::`, and the `host::` prefix of the clause names the \
             provider and is not part of it.",
            "Write `env`.",
        ] {
            assert!(
                msg.contains(fragment),
                "must carry `{fragment}`, got: {msg}"
            );
        }

        let msg = rejection_of(&manifest_with_host_imports("\"host::host\" = [\"h\"]\n"));
        assert!(
            msg.contains("invalid [host-imports] key `host::host`")
                && msg.contains("Write `host`."),
            "`host` is a legal import module, so `host::host` is told to write it, got: {msg}"
        );

        let msg = rejection_of(&manifest_with_host_imports(
            "\"env::v2\" = [\"clock_ms\"]\n",
        ));
        for fragment in [
            "invalid [host-imports] key `env::v2`",
            "that string is flat — one segment",
            "is listed as `env`.",
        ] {
            assert!(
                msg.contains(fragment),
                "must carry `{fragment}`, got: {msg}"
            );
        }
        assert!(
            !msg.contains("Write `"),
            "a key holding two names gets no guessed correction, got: {msg}"
        );
    }

    /// A key that is not an identifier, and an empty key, are refused naming the
    /// key.
    #[test]
    fn a_host_imports_key_that_is_not_an_identifier_is_refused() {
        for key in ["9x", "a-b", "a.b"] {
            let msg = rejection_of(&manifest_with_host_imports(&format!(
                "\"{key}\" = [\"clock_ms\"]\n"
            )));
            assert!(
                msg.contains(&format!("invalid [host-imports] key `{key}`"))
                    && msg.contains("ASCII identifier"),
                "`{key}` must be refused as a non-identifier key, got: {msg}"
            );
        }
        let msg = rejection_of(&manifest_with_host_imports("\"\" = [\"clock_ms\"]\n"));
        assert!(
            msg.contains("invalid [host-imports] key: the module name is empty"),
            "an empty key must be refused as empty, got: {msg}"
        );
    }

    /// A value that is not an array of strings is refused naming its key, in the
    /// diagnosis itself rather than only in the echoed source line — which, for
    /// an array spread over several lines, need not carry the key at all.
    ///
    /// The inline-table row is the future entry shape: an `infs` that predates it
    /// must refuse it by name rather than misread it.
    #[test]
    fn a_host_imports_value_of_the_wrong_shape_is_refused_naming_its_key() {
        for body in [
            "env = \"clock_ms\"\n",
            "env = 3\n",
            "env = { name = \"clock_ms\" }\n",
            "env = [{ name = \"clock_ms\" }]\n",
            "env = [\n    \"clock_ms\",\n    3,\n]\n",
        ] {
            let msg = rejection_of(&manifest_with_host_imports(body));
            assert!(
                msg.contains(
                    "invalid [host-imports] entry `env`: the value must be an array of strings"
                ),
                "the diagnosis must name the key for `{body}`, got: {msg}"
            );
        }
    }

    /// An empty array admits nothing under its key, which the flag it is
    /// forwarded as cannot even spell — so it is refused, with both remedies and
    /// the spelling of the policy its author may have meant.
    #[test]
    fn an_empty_host_imports_array_is_refused() {
        let msg = rejection_of(&manifest_with_host_imports("env = []\n"));
        for fragment in [
            "invalid [host-imports] entry `env = []`",
            "almost certainly a mistake",
            "Delete the key, or list the fields.",
            "a `[host-imports]` header with no keys under it",
        ] {
            assert!(
                msg.contains(fragment),
                "must carry `{fragment}`, got: {msg}"
            );
        }
    }

    /// A field that is not an identifier is refused naming the key and the field,
    /// and with the spelling to write where the field is a whole pair under the
    /// entry's own key — the shape every nearby surface prints.
    ///
    /// The rows without a correction are the ones it must not guess at: a pair
    /// naming another module may belong under another key, and a field that is
    /// merely malformed has no spelling to read off it.
    #[test]
    fn a_host_imports_field_that_is_not_an_identifier_is_refused() {
        let corrected = [
            ("env.clock_ms", Some("clock_ms")),
            ("host::env::clock_ms", Some("clock_ms")),
            ("env::clock_ms", Some("clock_ms")),
            ("fprime_core.telemetry", None),
            ("env.clock-ms", None),
            ("clock-ms", None),
            ("", None),
            ("9lives", None),
        ];
        for (field, bare) in corrected {
            let msg = rejection_of(&manifest_with_host_imports(&format!(
                "env = [\"{field}\"]\n"
            )));
            assert!(
                msg.contains(&format!(
                    "invalid [host-imports] entry `env`: `\"{field}\"` is not a host function name"
                )),
                "`{field}` must be refused naming the key and the field, got: {msg}"
            );
            match bare {
                Some(bare) => assert!(
                    msg.contains(&format!(
                        "digits or `_`. Write `\"{bare}\"`: the module is already the key."
                    )),
                    "`{field}` carries its own module, so the spelling is named, got: {msg}"
                ),
                None => assert!(
                    !msg.contains("Write `"),
                    "`{field}` holds no spelling to read off, so none is guessed, got: {msg}"
                ),
            }
        }
    }

    /// A field listed twice under one module is refused naming both.
    #[test]
    fn a_duplicate_host_imports_field_is_refused() {
        let msg = rejection_of(&manifest_with_host_imports(
            "env = [\"clock_ms\", \"sleep_ms\", \"clock_ms\"]\n",
        ));
        assert!(
            msg.contains("invalid [host-imports] entry `env`: `\"clock_ms\"` is listed twice"),
            "a duplicate must be refused naming the key and the field, got: {msg}"
        );
    }

    /// With two malformed modules the first in key order is reported, whatever
    /// order the file wrote them in — the same contract the `[build]` checks keep.
    #[test]
    fn two_malformed_host_imports_modules_report_the_first_in_key_order() {
        let msg = rejection_of(&manifest_with_host_imports("zeta = []\nalpha = []\n"));
        assert!(
            msg.contains("`alpha = []`") && !msg.contains("`zeta = []`"),
            "the first module in key order is the one reported, got: {msg}"
        );
    }

    /// A `[wasm-dependencies]` key whose first segment is `host` is refused on the
    /// `infs` side with the reserved-segment reason and the host-import remedy,
    /// while a key merely containing or resembling the segment is an ordinary
    /// module name.
    #[test]
    fn validate_wasm_dependency_key_refuses_the_host_segment() {
        for key in ["host", "host::a", "host::a::b"] {
            let err = validate_wasm_dependency_key(key)
                .expect_err("the first segment `host` is reserved");
            let msg = err.to_string();
            for fragment in [
                format!("invalid [wasm-dependencies] key `{key}`"),
                "`host` is reserved as the first segment of a `use … from` clause".to_string(),
                "list the function under `[host-imports]`".to_string(),
                format!(
                    "Rename the module — and the `use … from` clause that binds it — to a name \
                     outside `host` only if you meant a linked `.wasm` module named `{key}`: a \
                     module path whose first segment is `host` is reserved in this position \
                     and no longer resolves to a file."
                ),
            ] {
                assert!(
                    msg.contains(&fragment),
                    "must carry `{fragment}`, got: {msg}"
                );
            }
        }
        for key in ["hostlib", "Host", "a::host", "host_io"] {
            assert!(
                validate_wasm_dependency_key(key).is_ok(),
                "`{key}` does not open with the reserved segment"
            );
        }
    }
}
