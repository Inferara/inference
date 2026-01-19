#![warn(clippy::pedantic)]

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

use crate::project::manifest::{InferenceToml, validate_project_name};
use crate::project::templates::{TemplateFile, default_template_files};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Creates a new Inference project with the given name.
///
/// This function:
/// 1. Validates the project name
/// 2. Creates the project directory
/// 3. Generates all template files
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

    std::fs::create_dir_all(&project_path)
        .with_context(|| format!("Failed to create project directory: {}", project_path.display()))?;

    let template_files = default_template_files(name);
    write_template_files(&project_path, &template_files)?;

    if init_git {
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
    let manifest_content = manifest.write()?;

    std::fs::write(&manifest_path, manifest_content)
        .with_context(|| format!("Failed to write manifest: {}", manifest_path.display()))?;

    if create_src {
        let src_dir = project_path.join("src");
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("Failed to create src directory: {}", src_dir.display()))?;

        let main_path = src_dir.join("main.inf");
        if !main_path.exists() {
            let main_content = "// Entry point for the Inference program\n\nfn main() -> i32 {\n    return 0;\n}\n";
            std::fs::write(&main_path, main_content)
                .with_context(|| format!("Failed to write main.inf: {}", main_path.display()))?;
        }
    }

    Ok(())
}

/// Writes template files to the project directory.
fn write_template_files(project_path: &Path, files: &[TemplateFile]) -> Result<()> {
    for file in files {
        let file_path = project_path.join(&file.path);

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        std::fs::write(&file_path, &file.content)
            .with_context(|| format!("Failed to write file: {}", file_path.display()))?;
    }

    Ok(())
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
                eprintln!("Warning: Failed to run git: {e}. Project created without git repository.");
            }
        }
    }
}

/// Infers the project name from a directory path.
fn infer_project_name(path: &Path) -> Result<String> {
    let canonical = path.canonicalize().with_context(|| {
        format!(
            "Failed to resolve directory path: {}",
            path.display()
        )
    })?;

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

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("infs_test_{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_create_project_success() {
        let parent = temp_dir();
        let result = create_project("my_project", Some(&parent), false);

        assert!(result.is_ok());
        let project_path = result.unwrap();
        assert!(project_path.exists());
        assert!(project_path.join("Inference.toml").exists());
        assert!(project_path.join("src").join("main.inf").exists());
        assert!(project_path.join("tests").join(".gitkeep").exists());
        assert!(project_path.join("proofs").join(".gitkeep").exists());
        assert!(project_path.join(".gitignore").exists());

        cleanup(&parent);
    }

    #[test]
    fn test_create_project_invalid_name() {
        let parent = temp_dir();
        let result = create_project("fn", Some(&parent), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("reserved"));

        cleanup(&parent);
    }

    #[test]
    fn test_create_project_directory_exists() {
        let parent = temp_dir();
        let existing = parent.join("existing");
        fs::create_dir_all(&existing).unwrap();

        let result = create_project("existing", Some(&parent), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        cleanup(&parent);
    }

    #[test]
    fn test_create_project_with_git() {
        let parent = temp_dir();
        let result = create_project("git_project", Some(&parent), true);

        assert!(result.is_ok());
        let project_path = result.unwrap();

        // Git directory may or may not exist depending on git availability
        // The function should not fail either way
        assert!(project_path.join("Inference.toml").exists());

        cleanup(&parent);
    }

    #[test]
    fn test_init_project_success() {
        let dir = temp_dir();
        let result = init_project(Some(&dir), Some("init_test"), true);

        assert!(result.is_ok());
        assert!(dir.join("Inference.toml").exists());
        assert!(dir.join("src").join("main.inf").exists());

        cleanup(&dir);
    }

    #[test]
    fn test_init_project_no_src() {
        let dir = temp_dir();
        let result = init_project(Some(&dir), Some("init_test"), false);

        assert!(result.is_ok());
        assert!(dir.join("Inference.toml").exists());
        assert!(!dir.join("src").exists());

        cleanup(&dir);
    }

    #[test]
    fn test_init_project_already_exists() {
        let dir = temp_dir();
        fs::write(dir.join("Inference.toml"), "content").unwrap();

        let result = init_project(Some(&dir), Some("test"), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        cleanup(&dir);
    }

    #[test]
    fn test_init_project_invalid_name() {
        let dir = temp_dir();
        let result = init_project(Some(&dir), Some("struct"), false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("reserved"));

        cleanup(&dir);
    }

    #[test]
    fn test_init_project_infers_name() {
        let parent = temp_dir();
        let dir = parent.join("my_inferred_project");
        fs::create_dir_all(&dir).unwrap();

        let result = init_project(Some(&dir), None, false);

        assert!(result.is_ok());

        let manifest_content = fs::read_to_string(dir.join("Inference.toml")).unwrap();
        assert!(manifest_content.contains("my_inferred_project"));

        cleanup(&parent);
    }

    #[test]
    fn test_write_template_files() {
        let dir = temp_dir();
        let files = vec![
            TemplateFile::new("file1.txt", "content1"),
            TemplateFile::new(PathBuf::from("subdir").join("file2.txt"), "content2"),
        ];

        let result = write_template_files(&dir, &files);

        assert!(result.is_ok());
        assert!(dir.join("file1.txt").exists());
        assert!(dir.join("subdir").join("file2.txt").exists());

        assert_eq!(fs::read_to_string(dir.join("file1.txt")).unwrap(), "content1");

        cleanup(&dir);
    }
}
