//! Minimal manifest discovery for deriving a project's analysis source root.
//!
//! An Inference project is rooted at an `Inference.toml` manifest. The compiler
//! front end (`infs`) compiles the conventional `src/main.inf` entry point, so a
//! project's **source root** — the directory every path-form `use` resolves
//! against — is `<manifest_dir>/src`. The IDE, by contrast, opens individual
//! files as their own analysis entries; to resolve a non-entry file's imports
//! exactly as the compiler would, it must use that same source root rather than
//! the opened file's own directory.
//!
//! This module gives ide-db just enough to derive that root: walk up to the
//! nearest manifest, confirm it is a well-formed Inference manifest, and return
//! `<manifest_dir>/src` when the opened file lives under it. It deliberately does
//! not model the whole manifest — issue #256 will extract a shared project-model
//! crate, at which point `infs` and this helper can converge on one
//! implementation. Until then this small piece keeps the IDE and CLI agreeing on
//! what a manifest means without ide-db depending on `apps/infs`.
//!
//! # v1 limitation
//!
//! Discovery reads the manifest from disk at the moment a file is analyzed. There
//! is no filesystem watch, so a manifest created or edited *after* a file was
//! opened is not observed until that file's analysis is next recomputed for
//! another reason (a `didChange`, or a closure event that evicts it). This
//! matches the rest of ide-db, which observes only the files an editor opens.

use std::path::{Path, PathBuf};

/// The manifest file name that roots an Inference project. Mirrors `infs`'s
/// `manifest::MANIFEST_FILE_NAME`.
pub const MANIFEST_FILE_NAME: &str = "Inference.toml";

/// The conventional source subdirectory under a project's manifest directory.
///
/// `infs` compiles `<manifest_dir>/src/main.inf` (its
/// `ProjectContext::entry_relative()`), so the compiler's source root — what
/// path-form `use` directives resolve against — is `<manifest_dir>/src`. There
/// is no configurable source directory today; the convention is fixed.
const SOURCE_DIR_NAME: &str = "src";

/// Derives the analysis source root for `file` from the nearest ancestor
/// manifest, or `None` when no manifest governs it.
///
/// Walks up from `file`'s directory to the nearest `Inference.toml` (nearest
/// wins, mirroring `infs` project discovery). When that manifest is a well-formed
/// Inference manifest and `file` lives under its source root
/// (`<manifest_dir>/src`), returns that root so a resilient walk resolves
/// `file`'s imports as the compiler would. Returns `None` when no manifest is
/// found, the nearest manifest is malformed, or `file` lies outside the source
/// root — leaving the caller to fall back to another strategy.
#[must_use = "the derived source root is the reason to call this"]
pub fn manifest_source_root(file: &Path) -> Option<PathBuf> {
    let manifest_dir = find_manifest_dir(file)?;
    // A file whose nearest manifest cannot be loaded is one `infs` could not build
    // from, so the IDE treats it as project-less rather than inventing a root from
    // an unusable file. The nearest manifest wins even when malformed — the walk
    // does not climb past it to an outer manifest, matching `infs`.
    if !manifest_declares_package(&manifest_dir.join(MANIFEST_FILE_NAME)) {
        return None;
    }
    let source_root = manifest_dir.join(SOURCE_DIR_NAME);
    file.starts_with(&source_root).then_some(source_root)
}

/// Walks up from `start`'s directory to the nearest directory containing an
/// `Inference.toml`, returning that directory. `start` is treated as a file — the
/// search begins at its parent — unless it is itself a directory. The nearest
/// ancestor wins. Returns `None` when the filesystem root is reached with no
/// manifest found.
///
/// Presence is decided by an `is_file` probe alone; validity is a separate
/// concern (see [`manifest_source_root`]). Mirrors `infs`'s
/// `manifest::find_manifest_dir`.
fn find_manifest_dir(start: &Path) -> Option<PathBuf> {
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

/// Whether `manifest_path` reads as a well-formed Inference manifest: valid TOML
/// declaring a `[package]` table with string `name` and `version` keys.
///
/// This is the minimum `infs`'s `InferenceToml::from_toml` requires to load a
/// project, so a file failing it is one `infs` could not build from. An
/// unreadable file, invalid TOML, or a missing/partial `[package]` table all read
/// as "not a usable manifest here". Unknown extra keys are ignored, so a manifest
/// carrying newer fields still validates.
fn manifest_declares_package(manifest_path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(manifest_path) else {
        return false;
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return false;
    };
    let Some(package) = table.get("package").and_then(toml::Value::as_table) else {
        return false;
    };
    package.get("name").and_then(toml::Value::as_str).is_some()
        && package.get("version").and_then(toml::Value::as_str).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory tree under the system temp dir, removed on drop.
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "inference-manifest-{tag}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&root).expect("create temp tree root");
            TempTree { root }
        }

        /// Writes `contents` to `<root>/<relative>`, creating parent directories,
        /// and returns the absolute path.
        fn write(&self, relative: &str, contents: &str) -> PathBuf {
            let dest = self.root.join(relative);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
            std::fs::write(&dest, contents).expect("write file");
            dest
        }

        /// The absolute path a relative name would occupy, without creating it.
        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const VALID_MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n";

    #[test]
    fn find_manifest_dir_in_same_directory() {
        let tree = TempTree::new("find-same");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        let file = tree.write("main.inf", "pub fn main() {}");

        let found = find_manifest_dir(&file).expect("manifest in the file's own dir");
        assert_eq!(found, tree.root);
    }

    #[test]
    fn find_manifest_dir_in_ancestor() {
        let tree = TempTree::new("find-ancestor");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        let file = tree.write("src/deep/nested/a.inf", "pub fn a() {}");

        let found = find_manifest_dir(&file).expect("manifest in an ancestor");
        assert_eq!(found, tree.root);
    }

    #[test]
    fn find_manifest_dir_none_up_to_root() {
        let tree = TempTree::new("find-none");
        // No manifest anywhere in the tree.
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            find_manifest_dir(&file).is_none(),
            "a tree with no manifest must yield None"
        );
    }

    #[test]
    fn find_manifest_dir_nearest_ancestor_wins() {
        let tree = TempTree::new("find-nearest");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        tree.write(&format!("inner/{MANIFEST_FILE_NAME}"), VALID_MANIFEST);
        let file = tree.write("inner/src/a.inf", "pub fn a() {}");

        let found = find_manifest_dir(&file).expect("a manifest is found");
        assert_eq!(
            found,
            tree.path("inner"),
            "the nearest ancestor manifest must win over an outer one"
        );
    }

    #[test]
    fn source_root_is_manifest_dir_join_src() {
        let tree = TempTree::new("root-src");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        let file = tree.write("src/lib/a.inf", "pub fn a() {}");

        assert_eq!(
            manifest_source_root(&file),
            Some(tree.path("src")),
            "the source root must be <manifest_dir>/src"
        );
    }

    #[test]
    fn source_root_for_the_entry_file_itself() {
        let tree = TempTree::new("root-entry");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        let entry = tree.write("src/main.inf", "pub fn main() {}");

        assert_eq!(
            manifest_source_root(&entry),
            Some(tree.path("src")),
            "the conventional entry lives directly under the source root"
        );
    }

    #[test]
    fn source_root_nearest_manifest_wins() {
        let tree = TempTree::new("root-nearest");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        tree.write(&format!("inner/{MANIFEST_FILE_NAME}"), VALID_MANIFEST);
        let file = tree.write("inner/src/lib/a.inf", "pub fn a() {}");

        assert_eq!(
            manifest_source_root(&file),
            Some(tree.path("inner").join("src")),
            "the source root must be derived from the nearest manifest"
        );
    }

    #[test]
    fn source_root_none_for_file_outside_src() {
        let tree = TempTree::new("root-outside");
        tree.write(MANIFEST_FILE_NAME, VALID_MANIFEST);
        // A file under the project root but not under its `src` source tree.
        let file = tree.write("outside/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "a file outside <manifest_dir>/src must fall through"
        );
    }

    #[test]
    fn source_root_none_without_manifest() {
        let tree = TempTree::new("root-nomanifest");
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "no manifest means no manifest-derived source root"
        );
    }

    #[test]
    fn source_root_none_for_malformed_manifest() {
        let tree = TempTree::new("root-malformed");
        // Present but not valid TOML.
        tree.write(MANIFEST_FILE_NAME, "this is = = not valid toml");
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "a malformed manifest must fall through, not panic"
        );
    }

    #[test]
    fn source_root_none_for_manifest_without_package() {
        let tree = TempTree::new("root-nopackage");
        // Valid TOML, but not an Inference manifest (no [package]).
        tree.write(MANIFEST_FILE_NAME, "[build]\ntarget = \"wasm32\"\n");
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "a manifest lacking [package] is not a usable project manifest"
        );
    }

    #[test]
    fn source_root_none_for_package_missing_required_keys() {
        let tree = TempTree::new("root-partial-package");
        // [package] present but missing the required `version`.
        tree.write(MANIFEST_FILE_NAME, "[package]\nname = \"demo\"\n");
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "a [package] without both name and version is not loadable by infs"
        );
    }

    #[test]
    fn source_root_ignores_unknown_manifest_keys() {
        let tree = TempTree::new("root-forward-compat");
        // Extra unknown keys must not defeat validation (forward compatibility).
        tree.write(
            MANIFEST_FILE_NAME,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nfuture-field = 42\n",
        );
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert_eq!(
            manifest_source_root(&file),
            Some(tree.path("src")),
            "unknown forward-compatible keys must not defeat manifest validation"
        );
    }

    #[test]
    fn source_root_none_when_manifest_file_is_a_directory() {
        // A directory literally named `Inference.toml` is not a manifest: the
        // `is_file` probe rejects it, so discovery finds no manifest and no root.
        let tree = TempTree::new("root-dir-named-manifest");
        std::fs::create_dir_all(tree.path(MANIFEST_FILE_NAME)).unwrap();
        let file = tree.write("src/a.inf", "pub fn a() {}");

        assert!(
            manifest_source_root(&file).is_none(),
            "a directory named Inference.toml must not count as a manifest"
        );
    }
}
