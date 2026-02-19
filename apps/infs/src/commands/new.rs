//! New project command for the infs CLI.
//!
//! Creates a new Inference project with a standard directory structure.
//!
//! ## Usage
//!
//! ```bash
//! infs new myproject                    # Create project in current directory
//! infs new myproject --no-git           # Skip git initialization
//! infs new myproject ./path             # Create in specified directory
//! ```
//!
//! ## Project Structure
//!
//! Creates the following structure:
//!
//! ```text
//! myproject/
//! +-- Inference.toml
//! +-- src/
//! |   +-- main.inf
//! +-- tests/
//! |   +-- .gitkeep
//! +-- proofs/
//! |   +-- .gitkeep
//! +-- .gitignore
//! ```

use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use crate::project::create_project;

/// Arguments for the `new` command.
#[derive(Args)]
pub struct NewArgs {
    /// Name of the project to create.
    ///
    /// Must start with a letter or underscore and contain only
    /// alphanumeric characters, underscores, or hyphens.
    /// Cannot be a reserved Inference keyword.
    pub name: String,

    /// Parent directory for the project (defaults to current directory).
    #[clap(default_value = ".")]
    pub path: PathBuf,

    /// Skip git repository initialization.
    ///
    /// By default, `infs new` initializes a git repository in the
    /// new project directory. Use this flag to create a project
    /// without git.
    #[clap(long = "no-git", action = clap::ArgAction::SetTrue)]
    pub no_git: bool,
}

/// Executes the `new` command.
///
/// Creates a new Inference project with the standard directory structure.
///
/// # Errors
///
/// Returns an error if:
/// - The project name is invalid (reserved word or invalid characters)
/// - The target directory already exists
/// - File creation fails
pub fn execute(args: &NewArgs) -> Result<()> {
    let init_git = !args.no_git;
    let parent = if args.path.as_os_str() == "." {
        None
    } else {
        Some(args.path.as_path())
    };

    let project_path = create_project(&args.name, parent, init_git)?;

    println!("Created project '{}'", args.name);
    println!();
    println!("Next steps:");
    println!("  cd {}", project_path.display());
    println!("  infs build src/main.inf --codegen -o");
    println!();
    println!("To learn more about Inference, visit:");
    println!("  https://inference-lang.org");

    Ok(())
}
