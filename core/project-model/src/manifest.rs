//! Minimal manifest discovery for deriving how the IDE analyzes a project file.
//!
//! An Inference project is rooted at an `Inference.toml` manifest. The compiler
//! front end (`infs`) compiles the conventional `src/main.inf` entry point, so a
//! project's **source root** — the directory every path-form `use` resolves
//! against — is `<manifest_dir>/src`. The IDE, by contrast, opens individual
//! files as their own analysis entries; to resolve a non-entry file's imports
//! exactly as the compiler would, it must use that same source root rather than
//! the opened file's own directory. And to report what a build of the file would
//! report, it must analyze for the artifact the manifest describes: the target
//! its `[build] target` names and the shadow stack its `[memory]` table lays
//! out, since an analysis rule can measure a program against the runtime it is
//! built for and the stack it will run on.
//!
//! This module gives ide-db just enough to derive all three: walk up to the
//! nearest manifest, confirm it is a well-formed Inference manifest, and return
//! `<manifest_dir>/src`, the `[build] target` value and the `[memory]` keys when
//! the opened file lives under that root. It deliberately does not model the whole manifest — `infs`
//! owns the full `InferenceToml`, and a future change can let `infs` and this
//! helper converge on one implementation. Until then this small piece keeps the
//! IDE and CLI agreeing on what a manifest means without ide-db depending on
//! `apps/infs`.
//!
//! # v1 limitation
//!
//! Discovery reads the manifest from disk each time it is asked, and there is no
//! filesystem watch, so what a caller sees is the manifest as it was when it
//! asked. How often it asks is the caller's decision: ide-db asks when a
//! document's analysis has to be computed and keeps a manifest's answer until
//! the document is closed, so a manifest edited after that — a changed
//! `[build] target` or `[memory]` table included — is not observed until the
//! document is closed and reopened. This matches the rest of ide-db, which observes only the files an
//! editor opens.

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

/// What the manifest governing a file says about analyzing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSettings {
    /// `<manifest_dir>/src`, the directory every path-form `use` in the project
    /// resolves against.
    pub src_root: PathBuf,
    /// The `[build] target` value as written, or `None` when the manifest sets
    /// no string there.
    ///
    /// Not validated: the target vocabulary belongs to
    /// `inference-compiler-interface`, which this leaf crate does not depend
    /// on, so the caller resolves the name and decides what an unknown one
    /// means for it.
    pub build_target: Option<String>,
    /// The `[memory]` table's keys as written — every key unset when the
    /// manifest has no table — or `None` when the table holds something `infs`
    /// refuses to load before any layout is considered: a key it does not know,
    /// or a value that is not an integer a `u32` can hold.
    ///
    /// The keys are not validated as a layout, for the reason
    /// [`Self::build_target`] is not: whether they describe a memory a build can
    /// emit is decided jointly, by `inference-compiler-interface`, and the
    /// caller decides what a rejected layout means for it. What is decided here
    /// is whether the table can be read at all, because only here is an
    /// unreadable key still distinguishable from an absent one — and reading one
    /// as absent would resolve a layout the manifest does not describe, and one
    /// `infs` would never build.
    pub memory: Option<MemoryKeys>,
}

/// The keys of a manifest's `[memory]` table, each as written or unset.
///
/// Mirrors the fields of `infs`'s `MemoryConfig`. A key added there has to be
/// added here too, or a table that uses it reads as unusable — the IDE then
/// falls back to the default layout rather than guess at a key it does not know.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryKeys {
    /// `pages`: linear memory size in 64 KiB pages.
    pub pages: Option<u32>,
    /// `max-pages`: the most pages the memory may grow to.
    pub max_pages: Option<u32>,
    /// `stack-size`: shadow stack size in bytes.
    pub stack_size: Option<u32>,
}

/// Reads the settings that govern `file` from the nearest ancestor manifest, or
/// `None` when no manifest governs it.
///
/// Walks up from `file`'s directory to the nearest `Inference.toml` (nearest
/// wins, mirroring `infs` project discovery). When that manifest is a well-formed
/// Inference manifest and `file` lives under its source root
/// (`<manifest_dir>/src`), returns that root, the manifest's build target and its
/// memory keys, so a resilient walk resolves `file`'s imports as the compiler
/// would and analysis measures it against the target and the stack a build
/// would. Returns `None` when no manifest
/// is found, the nearest manifest is malformed, or `file` lies outside the source
/// root — leaving the caller to fall back to another strategy.
#[must_use = "the derived settings are the reason to call this"]
pub fn manifest_settings(file: &Path) -> Option<ManifestSettings> {
    let manifest_dir = find_manifest_dir(file)?;
    // A file whose nearest manifest cannot be loaded is one `infs` could not build
    // from, so the IDE treats it as project-less rather than inventing a root from
    // an unusable file. The nearest manifest wins even when malformed — the walk
    // does not climb past it to an outer manifest, matching `infs`.
    let manifest = read_package_manifest(&manifest_dir.join(MANIFEST_FILE_NAME))?;
    let src_root = manifest_dir.join(SOURCE_DIR_NAME);
    if !file.starts_with(&src_root) {
        return None;
    }
    let build_target = manifest
        .get("build")
        .and_then(toml::Value::as_table)
        .and_then(|build| build.get("target"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    Some(ManifestSettings {
        src_root,
        build_target,
        memory: memory_keys(&manifest),
    })
}

/// The keys of `manifest`'s `[memory]` table, or `None` when `infs` would refuse
/// to load the table: it is not a table, it holds a key `infs` does not know, or
/// a value is not an integer a `u32` can hold.
///
/// A manifest with no `[memory]` table has every key unset, exactly as `infs`
/// reads it. A value out of `u32` range is not a size `infs` would load, so it
/// makes the table unusable rather than being read as a truncation of itself.
fn memory_keys(manifest: &toml::Table) -> Option<MemoryKeys> {
    let Some(table) = manifest.get("memory") else {
        return Some(MemoryKeys::default());
    };
    let mut keys = MemoryKeys::default();
    for (key, value) in table.as_table()? {
        let slot = match key.as_str() {
            "pages" => &mut keys.pages,
            "max-pages" => &mut keys.max_pages,
            "stack-size" => &mut keys.stack_size,
            _ => return None,
        };
        *slot = Some(
            value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())?,
        );
    }
    Some(keys)
}

/// Derives the analysis source root for `file` from the nearest ancestor
/// manifest, or `None` when no manifest governs it.
///
/// The source-root half of [`manifest_settings`], which decides when a manifest
/// governs a file.
#[must_use = "the derived source root is the reason to call this"]
pub fn manifest_source_root(file: &Path) -> Option<PathBuf> {
    manifest_settings(file).map(|settings| settings.src_root)
}

/// Walks up from `start`'s directory to the nearest directory containing an
/// `Inference.toml`, returning that directory. `start` is treated as a file — the
/// search begins at its parent — unless it is itself a directory. The nearest
/// ancestor wins. Returns `None` when the filesystem root is reached with no
/// manifest found.
///
/// Presence is decided by an `is_file` probe alone; validity is a separate
/// concern (see [`manifest_settings`]). Mirrors `infs`'s
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

/// The manifest at `manifest_path` as a TOML table, when it reads as a
/// well-formed Inference manifest: valid TOML declaring a `[package]` table with
/// string `name` and `version` keys.
///
/// This is the minimum `infs`'s `InferenceToml::from_toml` requires to load a
/// project, so a file failing it is one `infs` could not build from. An
/// unreadable file, invalid TOML, or a missing/partial `[package]` table all read
/// as "not a usable manifest here". Unknown extra keys are ignored, so a manifest
/// carrying newer fields still validates.
fn read_package_manifest(manifest_path: &Path) -> Option<toml::Table> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let table = text.parse::<toml::Table>().ok()?;
    let package = table.get("package").and_then(toml::Value::as_table)?;
    let declares_package = package.get("name").and_then(toml::Value::as_str).is_some()
        && package.get("version").and_then(toml::Value::as_str).is_some();
    declares_package.then_some(table)
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

    /// `[build] target` comes back exactly as written, beside the source root
    /// the same manifest supplies.
    #[test]
    fn settings_carry_the_build_target_as_written() {
        let tree = TempTree::new("settings-target");
        tree.write(
            MANIFEST_FILE_NAME,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[build]\ntarget = \"spacewasm\"\n",
        );
        let file = tree.write("src/lib/a.inf", "pub fn a() {}");

        assert_eq!(
            manifest_settings(&file),
            Some(ManifestSettings {
                src_root: tree.path("src"),
                build_target: Some("spacewasm".to_string()),
                memory: Some(MemoryKeys::default()),
            })
        );
    }

    /// A manifest that names no target, whether it has no `[build]` table or a
    /// `[build]` table without the key, leaves the choice to the caller.
    #[test]
    fn settings_have_no_target_when_the_manifest_names_none() {
        for manifest in [
            VALID_MANIFEST.to_string(),
            format!("{VALID_MANIFEST}\n[build]\nmode = \"compile\"\n"),
        ] {
            let tree = TempTree::new("settings-no-target");
            tree.write(MANIFEST_FILE_NAME, &manifest);
            let file = tree.write("src/a.inf", "pub fn a() {}");

            let settings = manifest_settings(&file).expect("the manifest governs the file");
            assert_eq!(settings.src_root, tree.path("src"));
            assert_eq!(settings.build_target, None, "for manifest:\n{manifest}");
        }
    }

    /// The name is not validated here — the vocabulary is not this crate's —
    /// so an unknown one is returned as written. A target that is not a string
    /// at all is no name, and reads as none.
    #[test]
    fn settings_return_the_target_unvalidated() {
        for (value, expected) in [("\"wasm64\"", Some("wasm64")), ("7", None), ("[]", None)] {
            let tree = TempTree::new("settings-unvalidated");
            tree.write(
                MANIFEST_FILE_NAME,
                &format!("{VALID_MANIFEST}\n[build]\ntarget = {value}\n"),
            );
            let file = tree.write("src/a.inf", "pub fn a() {}");

            let settings = manifest_settings(&file).expect("the manifest governs the file");
            assert_eq!(settings.build_target.as_deref(), expected, "`target = {value}`");
        }
    }

    /// The nearest manifest wins for the target as it does for the source root:
    /// a nearer manifest naming none does not inherit an outer one's, and a
    /// nearer malformed one governs nothing rather than handing the file to the
    /// outer project.
    #[test]
    fn settings_come_from_the_nearest_manifest_only() {
        let spacewasm = format!("{VALID_MANIFEST}\n[build]\ntarget = \"spacewasm\"\n");

        let tree = TempTree::new("settings-nearest");
        tree.write(MANIFEST_FILE_NAME, &spacewasm);
        tree.write(&format!("inner/{MANIFEST_FILE_NAME}"), VALID_MANIFEST);
        let file = tree.write("inner/src/a.inf", "pub fn a() {}");
        assert_eq!(
            manifest_settings(&file),
            Some(ManifestSettings {
                src_root: tree.path("inner/src"),
                build_target: None,
                memory: Some(MemoryKeys::default()),
            })
        );

        let tree = TempTree::new("settings-nearest-malformed");
        tree.write(MANIFEST_FILE_NAME, &spacewasm);
        tree.write(&format!("src/lib/{MANIFEST_FILE_NAME}"), "not = = toml");
        let file = tree.write("src/lib/a.inf", "pub fn a() {}");
        assert_eq!(manifest_settings(&file), None);
    }

    /// A file under the project but outside its source root is not governed by
    /// the manifest, so it gets neither the root nor the target.
    #[test]
    fn settings_none_outside_the_source_root() {
        let tree = TempTree::new("settings-outside-src");
        tree.write(
            MANIFEST_FILE_NAME,
            &format!("{VALID_MANIFEST}\n[build]\ntarget = \"spacewasm\"\n"),
        );
        let file = tree.write("scratch/a.inf", "pub fn a() {}");
        assert_eq!(manifest_settings(&file), None);
    }

    /// Every `[memory]` key comes back as written, and each independently: a
    /// manifest that sets only one leaves the others to the caller's default,
    /// and one with no table leaves all three unset.
    #[test]
    fn settings_carry_the_memory_keys_as_written() {
        for (table, expected) in [
            (
                "[memory]\npages = 4\nmax-pages = 8\nstack-size = 131072\n",
                MemoryKeys {
                    pages: Some(4),
                    max_pages: Some(8),
                    stack_size: Some(131_072),
                },
            ),
            (
                "[memory]\npages = 2\n",
                MemoryKeys {
                    pages: Some(2),
                    ..MemoryKeys::default()
                },
            ),
            (
                "[memory]\nmax-pages = 8\n",
                MemoryKeys {
                    max_pages: Some(8),
                    ..MemoryKeys::default()
                },
            ),
            (
                "[memory]\nstack-size = 32768\n",
                MemoryKeys {
                    stack_size: Some(32_768),
                    ..MemoryKeys::default()
                },
            ),
            ("[memory]\n", MemoryKeys::default()),
            ("", MemoryKeys::default()),
        ] {
            let tree = TempTree::new("settings-memory");
            tree.write(MANIFEST_FILE_NAME, &format!("{VALID_MANIFEST}\n{table}"));
            let file = tree.write("src/a.inf", "pub fn a() {}");

            let settings = manifest_settings(&file).expect("the manifest governs the file");
            assert_eq!(settings.memory, Some(expected), "for table:\n{table}");
        }
    }

    /// The keys are not validated as a layout — a stack that does not fit its
    /// memory, or a zero, comes back as written for the caller to judge.
    #[test]
    fn settings_return_the_memory_keys_unvalidated_as_a_layout() {
        for (value, expected) in [("1000", 1000), ("0", 0), ("131072", 131_072)] {
            let tree = TempTree::new("settings-memory-unvalidated");
            tree.write(
                MANIFEST_FILE_NAME,
                &format!("{VALID_MANIFEST}\n[memory]\nstack-size = {value}\n"),
            );
            let file = tree.write("src/a.inf", "pub fn a() {}");

            let settings = manifest_settings(&file).expect("the manifest governs the file");
            assert_eq!(
                settings.memory.and_then(|keys| keys.stack_size),
                Some(expected),
                "`stack-size = {value}`"
            );
        }
    }

    /// A table `infs` would refuse before considering any layout is unusable as
    /// a whole, rather than read with the offending key dropped: dropping
    /// `max-pages = "8"` from `pages = 4, stack-size = 131072` would resolve a
    /// 128 KiB stack for a project `infs` refuses to build.
    #[test]
    fn a_memory_table_infs_would_refuse_is_unusable_as_a_whole() {
        for table in [
            "[memory]\npages = 4\nmax-pages = \"8\"\nstack-size = 131072\n",
            "[memory]\nstack-size = -16\n",
            "[memory]\nstack-size = 4294967296\n",
            "[memory]\npages = 1.5\n",
            "[memory]\npages = 4\nstack_size = 131072\n",
            "[memory]\npage = 2\n",
        ]
        .map(|table| format!("{VALID_MANIFEST}\n{table}"))
        // A root key, so it precedes the `[package]` header it would otherwise
        // be read as part of.
        .into_iter()
        .chain([format!("memory = 5\n{VALID_MANIFEST}")])
        {
            let tree = TempTree::new("settings-memory-refused");
            tree.write(MANIFEST_FILE_NAME, &table);
            let file = tree.write("src/a.inf", "pub fn a() {}");

            let settings = manifest_settings(&file).expect("the manifest governs the file");
            assert_eq!(settings.memory, None, "for table:\n{table}");
        }
    }
}
