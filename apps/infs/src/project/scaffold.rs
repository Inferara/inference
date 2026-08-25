//! Project scaffolding operations.
//!
//! This module provides functions to create new Inference projects and
//! initialize existing directories as Inference projects.
//!
//! ## Creating a New Project
//!
//! Use [`create_project`] to create a new project directory with all
//! necessary files.
//!
//! ## Initializing an Existing Directory
//!
//! Use [`init_project`] to initialize the current directory as an
//! Inference project without creating a new directory.

use crate::project::manifest::{InferenceToml, detect_infc_version, validate_project_name};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Creates a new Inference project with the given name.
///
/// This function:
/// 1. Validates the project name
/// 2. Creates the project directory
/// 3. Generates all project files
/// 4. Optionally initializes a git repository
///
/// # Arguments
///
/// * `name` - The project name (used for directory and manifest)
/// * `parent_path` - Optional parent directory (defaults to current directory)
/// * `init_git` - Whether to initialize a git repository
///
/// # Returns
///
/// The path to the created project directory.
///
/// # Errors
///
/// Returns an error if:
/// - The project name is invalid
/// - The target directory already exists
/// - File creation fails
pub fn create_project(name: &str, parent_path: Option<&Path>, init_git: bool) -> Result<PathBuf> {
    validate_project_name(name)?;

    let parent = parent_path.unwrap_or_else(|| Path::new("."));
    let project_path = parent.join(name);

    if project_path.exists() {
        bail!(
            "Directory '{}' already exists. Choose a different name or delete the existing directory.",
            project_path.display()
        );
    }

    std::fs::create_dir_all(&project_path).with_context(|| {
        format!(
            "Failed to create project directory: {}",
            project_path.display()
        )
    })?;

    write_project_files(&project_path, name)?;

    if init_git {
        write_git_files(&project_path)?;
        init_git_repository(&project_path);
    }

    Ok(project_path)
}

/// Initializes an existing directory as an Inference project.
///
/// This function creates manifest and optionally source files in an
/// existing directory without creating a new parent directory.
///
/// # Arguments
///
/// * `path` - The directory to initialize (defaults to current directory)
/// * `name` - Optional project name (defaults to directory name)
/// * `create_src` - Whether to create src/main.inf
///
/// # Errors
///
/// Returns an error if:
/// - The project name is invalid
/// - The manifest already exists
/// - File creation fails
pub fn init_project(path: Option<&Path>, name: Option<&str>, create_src: bool) -> Result<()> {
    let project_path = path.unwrap_or_else(|| Path::new("."));

    let project_name = match name {
        Some(n) => n.to_string(),
        None => infer_project_name(project_path)?,
    };

    validate_project_name(&project_name)?;

    let manifest_path = project_path.join("Inference.toml");
    if manifest_path.exists() {
        bail!(
            "Inference.toml already exists in '{}'. This directory is already an Inference project.",
            project_path.display()
        );
    }

    let manifest = InferenceToml::new(&project_name);
    manifest.write_to_file(&manifest_path)?;

    if create_src {
        let src_dir = project_path.join("src");
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("Failed to create src directory: {}", src_dir.display()))?;

        let main_path = src_dir.join("main.inf");
        if !main_path.exists() {
            std::fs::write(&main_path, main_inf_content())
                .with_context(|| format!("Failed to write main.inf: {}", main_path.display()))?;
        }
    }

    // If git is initialized, create git-related files (without overwriting existing ones)
    if project_path.join(".git").exists() {
        write_git_files_if_missing(project_path)?;
    }

    Ok(())
}

/// Writes git-related files only if they don't already exist.
fn write_git_files_if_missing(project_path: &Path) -> Result<()> {
    let tests_gitkeep = project_path.join("tests").join(".gitkeep");
    if !tests_gitkeep.exists() {
        std::fs::create_dir_all(project_path.join("tests"))
            .with_context(|| "Failed to create tests directory")?;
        std::fs::write(&tests_gitkeep, "").with_context(|| "Failed to write tests/.gitkeep")?;
    }

    let proofs_gitkeep = project_path.join("proofs").join(".gitkeep");
    if !proofs_gitkeep.exists() {
        std::fs::create_dir_all(project_path.join("proofs"))
            .with_context(|| "Failed to create proofs directory")?;
        std::fs::write(&proofs_gitkeep, "").with_context(|| "Failed to write proofs/.gitkeep")?;
    }

    let gitignore_path = project_path.join(".gitignore");
    if !gitignore_path.exists() {
        std::fs::write(&gitignore_path, gitignore_content())
            .with_context(|| "Failed to write .gitignore")?;
    }

    Ok(())
}

/// Writes core project files to the project directory.
fn write_project_files(project_path: &Path, project_name: &str) -> Result<()> {
    let manifest_path = project_path.join("Inference.toml");
    std::fs::write(&manifest_path, manifest_content(project_name)).with_context(|| {
        format!(
            "Failed to write Inference.toml: {}",
            manifest_path.display()
        )
    })?;

    let src_dir = project_path.join("src");
    std::fs::create_dir_all(&src_dir)
        .with_context(|| format!("Failed to create src directory: {}", src_dir.display()))?;

    let main_path = src_dir.join("main.inf");
    std::fs::write(&main_path, main_inf_content())
        .with_context(|| format!("Failed to write main.inf: {}", main_path.display()))?;

    let tests_dir = project_path.join("tests");
    std::fs::create_dir_all(&tests_dir)
        .with_context(|| format!("Failed to create tests directory: {}", tests_dir.display()))?;

    let proofs_dir = project_path.join("proofs");
    std::fs::create_dir_all(&proofs_dir).with_context(|| {
        format!(
            "Failed to create proofs directory: {}",
            proofs_dir.display()
        )
    })?;

    Ok(())
}

/// Writes git-related files (.gitignore, .gitkeep).
fn write_git_files(project_path: &Path) -> Result<()> {
    std::fs::write(project_path.join("tests").join(".gitkeep"), "")
        .with_context(|| "Failed to write tests/.gitkeep")?;
    std::fs::write(project_path.join("proofs").join(".gitkeep"), "")
        .with_context(|| "Failed to write proofs/.gitkeep")?;

    let gitignore_path = project_path.join(".gitignore");
    std::fs::write(&gitignore_path, gitignore_content())
        .with_context(|| format!("Failed to write .gitignore: {}", gitignore_path.display()))?;

    Ok(())
}

/// Generates the content for `Inference.toml`.
///
/// The `[build] mode` field is emitted explicitly because it is load-bearing
/// in project mode (`infs build`/`run` consume it), so the scaffolded file must
/// produce a value the loader reads back consistently — the round-trip is
/// covered by a test. `target`/`optimize` stay commented because they are not
/// yet consumed (writing them would imply they work). `[build.wasm-opt]` stays
/// commented because it is opt-in and requires an external `wasm-opt` binary
/// the project may not have installed. `[memory]` stays commented because its
/// defaults live in code and an uncommented table would forward flags to `infc`
/// on every build, gating a scaffolded project on a newer compiler ABI than it
/// needs. `[verification] output-dir` stays
/// commented because its default (`proofs/`) lives in code and is honored only
/// in proof mode.
fn manifest_content(project_name: &str) -> String {
    let infc_version = detect_infc_version();
    format!(
        r#"[package]
name = "{project_name}"
version = "0.1.0"
infc_version = "{infc_version}"

# Optional fields:
# description = "A brief description of the project"
# authors = ["Your Name <you@example.com>"]
# license = "MIT"

# [dependencies]
# Future: package dependencies
# std = "0.1"

[build]
# Compilation mode: "compile" (executable WASM) or "proof" (Rocq specs).
mode = "compile"
# Post-MVP WebAssembly proposals to opt into. Empty (the default) emits pure
# WebAssembly 1.0; "bulk-memory" lets codegen use memory.copy/memory.fill.
# Changing this changes the instruction set of every artifact, in both modes.
# wasm-features = ["bulk-memory"]
# Not yet consumed:
# target = "wasm32"
# optimize = "release"

# [build.wasm-opt]
# Post-build optimization of the executable via Binaryen wasm-opt. Uncomment to
# enable; `auto-install` downloads the `wasm-opt` binary on first use when it is
# not already present (equivalent to `infs component add wasm-opt`).
# enabled = true
# level = "3"
# auto-install = true

# [memory]
# Linear memory of the emitted module. Either key may be given alone; the other
# keeps its default. `stack-size` also sets the budget A036 measures call-chain
# frame usage against, so a smaller stack tightens that diagnostic.
# pages = 1
# stack-size = 65536

# [verification]
# Output directory for proof artifacts (honored only in proof mode).
# Defaults to "proofs/".
# output-dir = "proofs/"
"#
    )
}

/// Generates the content for `src/main.inf`.
fn main_inf_content() -> String {
    String::from(
        r"// Entry point for the Inference program

pub fn main() -> i32 {
    return 0;
}
",
    )
}

/// Generates the content for `.gitignore`.
///
/// `proofs/` is a tracked directory (via `proofs/.gitkeep`), but proof-mode
/// builds with the default `[verification] output-dir = "proofs/"` write
/// generated `.wasm`/`.v` artifacts there. We ignore those generated artifacts
/// by extension while keeping the directory (and any hand-authored files)
/// tracked — rather than ignoring all of `/proofs/`, which would drop
/// `.gitkeep` and any curated proof sources.
///
/// The Rocq patterns are deliberately unanchored where the proof patterns are
/// anchored to `/proofs/`. A `.v` is something a person can write, so where it
/// lives decides whether it is output; `coqc`'s own products have no
/// hand-authored form anywhere, and it writes them beside whichever source it
/// was handed rather than into an output directory.
fn gitignore_content() -> String {
    String::from(
        r"# Build outputs
/out/
/target/

# Generated proof artifacts (proof-mode output-dir defaults to proofs/).
# Keep proofs/ and curated sources tracked; ignore only generated files.
/proofs/*.wasm
/proofs/*.v

# Rocq build outputs, written beside the `.v` that `coqc` is handed.
*.vo
*.vok
*.vos
*.glob
*.aux
.lia.cache
.coq-native/

# IDE and editor files
.idea/
.vscode/
*.swp
*.swo
*~

# OS files
.DS_Store
Thumbs.db
",
    )
}

/// Initializes a git repository in the project directory.
///
/// This function logs a warning if git initialization fails rather than
/// returning an error, as git is optional.
fn init_git_repository(project_path: &Path) {
    let result = Command::new("git")
        .args(["init"])
        .current_dir(project_path)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            // Silently succeed
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Warning: git init failed: {}. Project created without git repository.",
                stderr.trim()
            );
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Warning: git not found. Project created without git repository.");
            } else {
                eprintln!(
                    "Warning: Failed to run git: {e}. Project created without git repository."
                );
            }
        }
    }
}

/// Infers the project name from a directory path.
fn infer_project_name(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve directory path: {}", path.display()))?;

    canonical
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("Could not determine project name from directory path"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Creates a scratch directory for a scaffolding test.
    ///
    /// The returned `TempDir` owns the directory and deletes it on drop, so
    /// callers must keep it bound for the whole test. The name comes from the
    /// operating system's exclusive-create loop, so it is unique against both
    /// parallel test threads and concurrent test processes.
    fn temp_dir() -> assert_fs::TempDir {
        assert_fs::TempDir::new().unwrap()
    }

    #[test]
    fn test_create_project_success() {
        let parent = temp_dir();
        let result = create_project("my_project", Some(parent.path()), false);

        assert!(result.is_ok());
        let project_path = result.unwrap();
        assert!(project_path.exists());
        assert!(project_path.join("Inference.toml").exists());
        assert!(project_path.join("src").join("main.inf").exists());
        assert!(project_path.join("tests").exists());
        assert!(project_path.join("proofs").exists());
        // With --no-git, git-related files should NOT be created
        assert!(!project_path.join("tests").join(".gitkeep").exists());
        assert!(!project_path.join("proofs").join(".gitkeep").exists());
        assert!(!project_path.join(".gitignore").exists());
    }

    #[test]
    fn test_create_project_with_git_creates_gitignore() {
        let parent = temp_dir();
        let result = create_project("git_enabled_project", Some(parent.path()), true);

        assert!(result.is_ok());
        let project_path = result.unwrap();
        assert!(project_path.exists());
        assert!(project_path.join("Inference.toml").exists());
        assert!(project_path.join("src").join("main.inf").exists());
        assert!(project_path.join("tests").join(".gitkeep").exists());
        assert!(project_path.join("proofs").join(".gitkeep").exists());
        assert!(project_path.join(".gitignore").exists());
    }

    #[test]
    fn test_create_project_invalid_name() {
        let parent = temp_dir();
        let result = create_project("fn", Some(parent.path()), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("reserved"));
    }

    #[test]
    fn test_create_project_directory_exists() {
        let parent = temp_dir();
        let existing = parent.path().join("existing");
        fs::create_dir_all(&existing).unwrap();

        let result = create_project("existing", Some(parent.path()), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_create_project_with_git() {
        let parent = temp_dir();
        let result = create_project("git_project", Some(parent.path()), true);

        assert!(result.is_ok());
        let project_path = result.unwrap();

        // Git directory may or may not exist depending on git availability
        // The function should not fail either way
        assert!(project_path.join("Inference.toml").exists());
    }

    #[test]
    fn test_init_project_success() {
        let dir = temp_dir();
        let result = init_project(Some(dir.path()), Some("init_test"), true);

        assert!(result.is_ok());
        assert!(dir.path().join("Inference.toml").exists());
        assert!(dir.path().join("src").join("main.inf").exists());
    }

    #[test]
    fn test_init_project_no_src() {
        let dir = temp_dir();
        let result = init_project(Some(dir.path()), Some("init_test"), false);

        assert!(result.is_ok());
        assert!(dir.path().join("Inference.toml").exists());
        assert!(!dir.path().join("src").exists());
    }

    #[test]
    fn test_init_project_already_exists() {
        let dir = temp_dir();
        fs::write(dir.path().join("Inference.toml"), "content").unwrap();

        let result = init_project(Some(dir.path()), Some("test"), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_init_project_invalid_name() {
        let dir = temp_dir();
        let result = init_project(Some(dir.path()), Some("struct"), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("reserved"));
    }

    #[test]
    fn test_init_project_infers_name() {
        let parent = temp_dir();
        let dir = parent.path().join("my_inferred_project");
        fs::create_dir_all(&dir).unwrap();

        let result = init_project(Some(&dir), None, false);

        assert!(result.is_ok());

        let manifest_content = fs::read_to_string(dir.join("Inference.toml")).unwrap();
        assert!(manifest_content.contains("my_inferred_project"));
    }

    #[test]
    fn test_manifest_contains_project_name() {
        let content = manifest_content("my_awesome_project");
        assert!(content.contains("my_awesome_project"));
        assert!(content.contains("version = \"0.1.0\""));
        assert!(content.contains("infc_version = \""));
    }

    #[test]
    fn test_main_inf_has_entry_point() {
        let content = main_inf_content();
        assert!(content.contains("fn main()"));
        assert!(content.contains("return"));
    }

    #[test]
    fn test_gitignore_excludes_build_dirs() {
        let content = gitignore_content();
        assert!(content.contains("/out/"));
        assert!(content.contains("/target/"));
    }

    #[test]
    fn test_gitignore_ignores_generated_proof_artifacts_but_keeps_dir() {
        // The proofs/ directory stays tracked (.gitkeep), only generated
        // artifacts are ignored.
        let content = gitignore_content();
        assert!(content.contains("/proofs/*.wasm"));
        assert!(content.contains("/proofs/*.v"));
        assert!(
            !content.contains("/proofs/\n") && !content.contains("/proofs/ "),
            "must not ignore the whole proofs/ directory (would drop .gitkeep)"
        );
    }

    /// The scaffolded ignore rules must hide what `coqc` writes, not only what
    /// `infs` emits. Checking a generated proof by hand leaves a `.vo`, a
    /// `.glob` and a dot-prefixed `.<module>.aux` beside every `.v` compiled,
    /// which is three files per module dropped into the project.
    ///
    /// This runs `git status` against a real scaffolded repository instead of
    /// matching the template text, because the failure mode is a pattern that
    /// reads correctly and matches nothing: Rocq names its auxiliary file
    /// `.main.aux`, not `main.aux`, and anchoring the rules under `/proofs/`
    /// would miss a proof compiled anywhere else. Listing `src/main.inf` is
    /// what keeps the assertion honest — without it a `git status` that
    /// reported nothing at all would pass.
    #[test]
    fn test_gitignore_hides_rocq_build_artifacts() {
        let parent = temp_dir();
        let project = create_project("rocq_artifacts", Some(parent.path()), true)
            .expect("scaffold a git-enabled project");
        assert!(
            project.join(".git").exists(),
            "this test needs a real repository, and `git init` produced none \
             (is `git` on PATH?)"
        );

        // One full set of `coqc` products under the default output directory,
        // plus a proof compiled outside it, plus the two artifacts a solver
        // cache and native compilation add at the project root.
        let artifacts = [
            "proofs/main.vo",
            "proofs/main.vok",
            "proofs/main.vos",
            "proofs/main.glob",
            "proofs/.main.aux",
            "src/hand_written.vo",
            "src/hand_written.glob",
            ".lia.cache",
            ".coq-native/Nmain.cmi",
        ];
        for rel in artifacts {
            let path = project.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap_or_else(|e| panic!("write {rel}: {e}"));
        }

        let output = Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=all"])
            .current_dir(&project)
            .output()
            .expect("run git status in the scaffolded project");
        assert!(output.status.success(), "git status failed in {project:?}");
        let status = String::from_utf8_lossy(&output.stdout);

        // Porcelain v1 is `XY <path>`; compare whole paths so a leaked
        // `main.vok` is not reported against `main.vo`, which is its prefix.
        let reported: Vec<&str> = status
            .lines()
            .filter_map(|line| line.get(3..))
            .map(str::trim)
            .collect();

        for rel in artifacts {
            assert!(
                !reported.contains(&rel),
                "`{rel}` is a coqc build artifact and must not reach git status:\n{status}"
            );
        }
        assert!(
            reported.contains(&"src/main.inf"),
            "git status reported no scaffolded source, so the assertions above \
             proved nothing:\n{status}"
        );
    }

    #[test]
    fn test_scaffolded_manifest_roundtrips_through_load() {
        // The scaffolded string template must parse + validate and load back
        // with the load-bearing `[build] mode` == "compile".
        let content = manifest_content("roundtrip_demo");
        let manifest = InferenceToml::from_toml(&content)
            .expect("scaffolded manifest must parse and validate");
        assert_eq!(manifest.package.name, "roundtrip_demo");
        assert_eq!(manifest.build.mode, "compile");
    }

    #[test]
    fn test_scaffolded_manifest_emits_explicit_build_mode() {
        let content = manifest_content("demo");
        assert!(
            content.contains("[build]") && content.contains("mode = \"compile\""),
            "scaffold must emit an explicit [build] mode"
        );
    }
}
