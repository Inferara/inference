#![warn(clippy::pedantic)]

//! Project template generation.
//!
//! This module provides functions to generate the default file structure
//! for new Inference projects.
//!
//! ## Default Project Structure
//!
//! ```text
//! myproject/
//! ├── Inference.toml
//! ├── src/
//! │   └── main.inf
//! ├── tests/
//! │   └── .gitkeep
//! ├── proofs/
//! │   └── .gitkeep
//! └── .gitignore
//! ```

use std::path::PathBuf;

/// A file to be created as part of project scaffolding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFile {
    /// Relative path from project root.
    pub path: PathBuf,
    /// File content.
    pub content: String,
}

impl TemplateFile {
    /// Creates a new template file.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

/// Generates the default template files for a new Inference project.
///
/// # Arguments
///
/// * `project_name` - The name of the project (used in manifest)
///
/// # Returns
///
/// A vector of template files to be created in the project directory.
#[must_use]
pub fn default_template_files(project_name: &str) -> Vec<TemplateFile> {
    vec![
        TemplateFile::new("Inference.toml", manifest_content(project_name)),
        TemplateFile::new(
            PathBuf::from("src").join("main.inf"),
            main_inf_content(),
        ),
        TemplateFile::new(PathBuf::from("tests").join(".gitkeep"), String::new()),
        TemplateFile::new(PathBuf::from("proofs").join(".gitkeep"), String::new()),
        TemplateFile::new(".gitignore", gitignore_content()),
    ]
}

/// Generates the content for `Inference.toml`.
fn manifest_content(project_name: &str) -> String {
    format!(
        r#"[package]
name = "{project_name}"
version = "0.1.0"
manifest_version = 1

# Optional fields:
# description = "A brief description of the project"
# authors = ["Your Name <you@example.com>"]
# license = "MIT"
"#
    )
}

/// Generates the content for `src/main.inf`.
fn main_inf_content() -> String {
    String::from(
        r"// Entry point for the Inference program

fn main() -> i32 {
    return 0;
}
",
    )
}

/// Generates the content for `.gitignore`.
fn gitignore_content() -> String {
    String::from(
        r"# Build outputs
/out/
/target/

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_template_creates_all_files() {
        let files = default_template_files("test_project");

        let paths: Vec<_> = files.iter().map(|f| f.path.to_string_lossy().to_string()).collect();

        assert!(paths.iter().any(|p| p == "Inference.toml"));
        assert!(paths.iter().any(|p| p.ends_with("main.inf")));
        assert!(paths.iter().any(|p| p.contains("tests")));
        assert!(paths.iter().any(|p| p.contains("proofs")));
        assert!(paths.iter().any(|p| p == ".gitignore"));
    }

    #[test]
    fn test_manifest_contains_project_name() {
        let files = default_template_files("my_awesome_project");
        let manifest = files.iter().find(|f| f.path.to_string_lossy() == "Inference.toml").unwrap();

        assert!(manifest.content.contains("my_awesome_project"));
        assert!(manifest.content.contains("version = \"0.1.0\""));
        assert!(manifest.content.contains("manifest_version = 1"));
    }

    #[test]
    fn test_main_inf_has_entry_point() {
        let files = default_template_files("project");
        let main = files.iter().find(|f| f.path.to_string_lossy().ends_with("main.inf")).unwrap();

        assert!(main.content.contains("fn main()"));
        assert!(main.content.contains("return"));
    }

    #[test]
    fn test_gitignore_excludes_build_dirs() {
        let files = default_template_files("project");
        let gitignore = files.iter().find(|f| f.path.to_string_lossy() == ".gitignore").unwrap();

        assert!(gitignore.content.contains("/out/"));
        assert!(gitignore.content.contains("/target/"));
    }

    #[test]
    fn test_gitkeep_files_are_empty() {
        let files = default_template_files("project");

        for file in &files {
            if file.path.to_string_lossy().ends_with(".gitkeep") {
                assert!(file.content.is_empty(), "gitkeep should be empty");
            }
        }
    }

    #[test]
    fn test_template_file_new() {
        let file = TemplateFile::new("path/to/file.txt", "content here");
        assert_eq!(file.path, PathBuf::from("path/to/file.txt"));
        assert_eq!(file.content, "content here");
    }
}
