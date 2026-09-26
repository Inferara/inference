#![warn(clippy::pedantic)]

//! Every `[profile]` table in the checkout sits in a workspace root's manifest,
//! but the one `tools/wat-fmt` keeps for its published package ([`exempt`]).
//!
//! Cargo reads `[profile]` tables only from the manifest of a workspace root. A
//! member that declares one gets a warning at the top of every build and no
//! other effect, which is how `infs` shipped every release without the size
//! settings its own manifest declared. A member's settings belong in the root
//! manifest as `[profile.<name>.package.<member>]`, where Cargo applies them to
//! that package's own code.
//!
//! A manifest is a root when it declares a `[workspace]` table, as the
//! checkout's own does, and as the fuzz crate under `core/wasm-linker/fuzz` and
//! the library under `tests/test_data/wasmlib/rustlib-src` do to stand outside
//! this workspace ([`independent_roots`]); their profiles apply to their own
//! builds. A package the root lists in `workspace.exclude` would be a root
//! without one, so the test also holds the root to excluding nothing. It reads
//! every `Cargo.toml` in the working tree rather than the root's member list,
//! so a package no member glob matches is held to the same rule. Only files in
//! the checkout are read, so the test runs offline and on every platform.

use std::path::{Path, PathBuf};

/// The repository checkout this crate sits in.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The one manifest whose ignored profile is left in place, relative to the
/// checkout.
///
/// `wat-fmt` is published to crates.io on its own, and the manifest it is
/// published with keeps its `[profile.release]`: Cargo applies it when that
/// package is built as a root of its own, and ignores it here as it ignored
/// `infs`'s. Whether it stays is a decision about that crate's releases.
fn exempt() -> PathBuf {
    Path::new("tools").join("wat-fmt").join("Cargo.toml")
}

/// The manifests besides the checkout's own that declare a `[workspace]` table
/// and a profile with it, relative to the checkout.
fn independent_roots() -> [PathBuf; 2] {
    let fuzz = Path::new("core").join("wasm-linker").join("fuzz");
    let rustlib = Path::new("tests")
        .join("test_data")
        .join("wasmlib")
        .join("rustlib-src");
    [fuzz.join("Cargo.toml"), rustlib.join("Cargo.toml")]
}

/// Every `Cargo.toml` under `dir`, never following a symbolic link.
///
/// The walk does not enter build output, npm packages, or a directory whose
/// name starts with a dot, such as version control, CI and Cargo configuration,
/// or local tool state, none of which holds a package.
fn manifest_paths(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
    {
        let entry = entry.expect("a directory entry");
        let path = entry.path();
        let kind = entry
            .file_type()
            .unwrap_or_else(|e| panic!("the type of {} must be readable: {e}", path.display()));
        let name = entry.file_name();
        let skipped = matches!(name.to_str(), Some("target" | "node_modules"))
            || name.as_encoded_bytes().starts_with(b".");
        if kind.is_dir() && !skipped {
            manifest_paths(&path, found);
        } else if kind.is_file() && name == "Cargo.toml" {
            found.push(path);
        }
    }
}

/// Whether Cargo ignores the profiles `manifest` declares: it has a `profile`
/// table and no `workspace` table, so it is not the manifest of a root.
fn declares_ignored_profiles(manifest: &toml::Table) -> bool {
    manifest.contains_key("profile") && !manifest.contains_key("workspace")
}

/// `text`, parsed as TOML.
fn table(text: &str) -> toml::Table {
    toml::from_str(text).unwrap_or_else(|e| panic!("{text:?} is not TOML: {e}"))
}

/// The manifest at `path`, parsed.
fn manifest(path: &Path) -> toml::Table {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{} is not TOML: {e}", path.display()))
}

/// No manifest in the checkout declares a profile Cargo ignores, but the one
/// [`exempt`] names, and that one still does.
///
/// Fails naming each manifest that declares a `[profile]` table without being a
/// workspace root, whose settings Cargo drops with only a warning, as it
/// dropped `infs`'s from every release. Fails too when the exempt manifest stops
/// declaring one, so its exemption goes with it; when the walk misses a
/// manifest this file names, so a narrowed walk cannot pass by reading less;
/// and when the root excludes a package, which the rule would misjudge.
#[test]
fn every_profile_is_declared_by_a_workspace_root() {
    let root = repository()
        .canonicalize()
        .expect("the checkout's root resolves");
    let mut found = Vec::new();
    manifest_paths(&root, &mut found);
    let found: Vec<PathBuf> = found
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap_or(path).to_path_buf())
        .collect();
    let named = [
        PathBuf::from("Cargo.toml"),
        Path::new("apps").join("infs").join("Cargo.toml"),
        exempt(),
    ];
    for expected in named.iter().chain(&independent_roots()) {
        assert!(
            found.contains(expected),
            "the walk found no {} under {}, so it is not reading the checkout",
            expected.display(),
            root.display()
        );
    }

    let offenders: Vec<String> = found
        .iter()
        .filter(|path| **path != exempt())
        .filter(|path| declares_ignored_profiles(&manifest(&root.join(path))))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "Cargo reads profiles only from a workspace root's manifest and ignores these, with a \
         warning: {offenders:?}. Set the package's options in the root Cargo.toml as \
         `[profile.<name>.package.<package>]` (`panic`, `lto` and `rpath` cannot be set per \
         package), or, for a package no member glob matches, make its manifest a root of its \
         own with a `[workspace]` table"
    );

    assert!(
        declares_ignored_profiles(&manifest(&root.join(exempt()))),
        "{} no longer declares a profile Cargo ignores, so its exemption goes too",
        exempt().display()
    );
    for independent in independent_roots() {
        let declared = manifest(&root.join(&independent));
        assert!(
            declared.contains_key("profile") && declared.contains_key("workspace"),
            "{} no longer declares both a profile and a `[workspace]` table, so this file's \
             account of it is stale",
            independent.display()
        );
    }

    let checkout = manifest(&root.join("Cargo.toml"));
    if let Some(excluded) = checkout
        .get("workspace")
        .and_then(|workspace| workspace.get("exclude"))
    {
        panic!(
            "the root Cargo.toml excludes {excluded}: an excluded package builds as a root of its \
             own without a `[workspace]` table, so its profiles apply, and the rule has to learn \
             to read the root's exclusions before it can judge them"
        );
    }
}

/// The rule finds a `profile` table in any form TOML writes one, and takes a
/// `workspace` table — not the `package.workspace` key a member names its root
/// with — as what makes a manifest a root.
///
/// Fails if the rule starts missing a profile a member declares, or starts
/// flagging one a root declares — either would leave the scan above reporting a
/// checkout that is not the one Cargo builds.
#[test]
fn the_rule_tells_a_root_from_a_member() {
    let package = "[package]\nname = \"p\"\nversion = \"0.0.1\"\n";
    for ignored in [
        format!("{package}[profile.release]\nopt-level = \"z\"\n"),
        format!("{package}[profile.release.package.p]\nstrip = true\n"),
        format!("{package}[profile.dev.package.\"*\"]\ndebug = false\n"),
        format!("{package}[profile.dev]\n"),
        format!("{package}[package.metadata.x]\n[profile.release]\nlto = true\n"),
        format!("profile.release.opt-level = \"z\"\n{package}"),
        format!("profile = {{ release = {{ opt-level = \"z\" }} }}\n{package}"),
        format!("{package}workspace = \"..\"\n[profile.release]\n"),
    ] {
        assert!(
            declares_ignored_profiles(&table(&ignored)),
            "a member's profile is ignored: {ignored:?}"
        );
    }

    for applied in [
        package.to_string(),
        format!("{package}[package.metadata.profile]\nrelease = true\n"),
        format!("{package}[workspace]\n[profile.release]\ndebug = 1\n"),
        format!("{package}[workspace]\nmembers = [\"a\"]\n[profile.release]\n"),
        format!("{package}[workspace.dependencies]\n[profile.release]\n"),
        format!("workspace.members = []\n{package}[profile.release]\n"),
    ] {
        assert!(
            !declares_ignored_profiles(&table(&applied)),
            "no ignored profile: {applied:?}"
        );
    }
}
