//! Project management module.
//!
//! This module provides functionality for creating and managing Inference
//! projects, including manifest handling and project scaffolding.
//!
//! ## Modules
//!
//! - [`manifest`] - Inference.toml parsing and validation
//! - [`scaffold`] - Project creation and initialization
//!
//! ## Key Types
//!
//! - [`manifest::InferenceToml`] - The manifest file structure
//! - [`ProjectContext`] - A discovered project: root, manifest, and entry point
//!
//! ## Project Discovery
//!
//! [`discover_and_load`] walks up from a starting directory to find the
//! project's `Inference.toml`, parses it, and resolves the conventional
//! `src/main.inf` entry point. This keeps the filesystem-walking and
//! manifest-loading logic out of the individual command modules.

pub mod manifest;
pub mod scaffold;

use anyhow::Result;
use std::path::{Path, PathBuf};

use manifest::{InferenceToml, discover_manifest};

pub use scaffold::{create_project, init_project};

/// A discovered Inference project.
///
/// Produced by [`discover_and_load`]. Holds everything project-mode `build`
/// and `run` need: the project root (the directory containing
/// `Inference.toml`), the parsed manifest, and the resolved entry-point path.
///
/// All paths are absolute so callers can use them regardless of the current
/// working directory; the project root is used as the working directory when
/// spawning `infc` so that `out/` lands at the root.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// Absolute path to the project root (the directory holding the manifest).
    pub root: PathBuf,

    /// The parsed `Inference.toml` manifest.
    ///
    /// Consumed by project-mode `build` to resolve the effective `[build] mode`
    /// and `[verification] output-dir` when forwarding to `infc`.
    pub manifest: InferenceToml,

    /// Absolute path to the conventional entry point, `<root>/src/main.inf`.
    ///
    /// This is the *expected* location; it is not guaranteed to exist. The
    /// `build`/`run` command paths report a remediation error when it is
    /// missing, since the manifest may legitimately exist before the entry
    /// point has been authored.
    pub entry_point: PathBuf,
}

impl ProjectContext {
    /// The conventional entry-point path relative to the project root:
    /// `src/main.inf`. Built through [`Path::join`] so the separator is
    /// platform-correct (never a literal `/`).
    #[must_use = "computes the entry-point path without side effects; discarding it is a bug"]
    pub fn entry_relative() -> PathBuf {
        Path::new("src").join("main.inf")
    }
}

/// Discovers the project containing `cwd` and loads its manifest.
///
/// Walks up from `cwd` to find `Inference.toml` (nearest ancestor wins),
/// parses it, and resolves the conventional `src/main.inf` entry point. The
/// returned [`ProjectContext`] carries absolute paths.
///
/// # Errors
///
/// Returns a remediation-style error if no manifest is found in `cwd` or any
/// ancestor, or if the discovered manifest fails to parse.
pub fn discover_and_load(cwd: &Path) -> Result<ProjectContext> {
    let manifest_path = discover_manifest(cwd)?;
    let manifest = InferenceToml::load(&manifest_path)?;

    let root = manifest_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let entry_point = root.join(ProjectContext::entry_relative());

    Ok(ProjectContext {
        root,
        manifest,
        entry_point,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use manifest::MANIFEST_FILE_NAME;

    /// Writes a minimal valid manifest and a `src/main.inf` under `root`.
    fn scaffold_minimal(root: &Path, name: &str) {
        let manifest = InferenceToml::new(name);
        manifest
            .write_to_file(&root.join(MANIFEST_FILE_NAME))
            .unwrap();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.inf"), "pub fn main() -> i32 { return 0; }\n").unwrap();
    }

    #[test]
    fn entry_relative_uses_platform_separator() {
        let rel = ProjectContext::entry_relative();
        // Joining components yields the platform separator; the string form
        // must never embed a literal forward slash on Windows.
        assert_eq!(rel, Path::new("src").join("main.inf"));
        assert_eq!(rel.file_name().unwrap(), "main.inf");
    }

    #[test]
    fn discover_and_load_from_root() {
        let dir = assert_fs::TempDir::new().unwrap();
        scaffold_minimal(dir.path(), "demo");

        let ctx = discover_and_load(dir.path()).unwrap();
        assert_eq!(ctx.manifest.package.name, "demo");
        assert_eq!(
            ctx.root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(ctx.entry_point, ctx.root.join("src").join("main.inf"));
        assert!(ctx.entry_point.exists());
    }

    #[test]
    fn discover_and_load_from_subdir() {
        let dir = assert_fs::TempDir::new().unwrap();
        scaffold_minimal(dir.path(), "demo");

        // Discover from the nested src directory; root must be the manifest dir.
        let from = dir.path().join("src");
        let ctx = discover_and_load(&from).unwrap();
        assert_eq!(
            ctx.root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
        assert!(ctx.entry_point.exists());
    }

    #[test]
    fn discover_and_load_entry_point_may_be_absent() {
        // A manifest can exist before src/main.inf is authored; discovery must
        // still succeed and point entry_point at the conventional location.
        let dir = assert_fs::TempDir::new().unwrap();
        InferenceToml::new("demo")
            .write_to_file(&dir.path().join(MANIFEST_FILE_NAME))
            .unwrap();

        let ctx = discover_and_load(dir.path()).unwrap();
        assert_eq!(ctx.entry_point, ctx.root.join("src").join("main.inf"));
        assert!(!ctx.entry_point.exists());
    }

    #[test]
    fn discover_and_load_errors_without_manifest() {
        let dir = assert_fs::TempDir::new().unwrap();
        let result = discover_and_load(dir.path());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains(MANIFEST_FILE_NAME),
            "error should mention the manifest file"
        );
    }
}
