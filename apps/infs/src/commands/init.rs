//! Init command for the infs CLI.
//!
//! Initializes an existing directory as an Inference project.
//!
//! ## Usage
//!
//! ```bash
//! infs init              # Initialize current directory
//! infs init myproject    # Initialize with explicit name
//! ```
//!
//! ## Behavior
//!
//! - Creates `Inference.toml` in the current directory
//! - Creates `src/main.inf` with a basic entry point
//! - If `.git/` exists, creates `.gitignore` and `.gitkeep` files (without overwriting)
//! - Project name defaults to the directory name if not provided

use anyhow::Result;
use clap::Args;

use crate::project::init_project;

/// Arguments for the `init` command.
#[derive(Args)]
pub struct InitArgs {
    /// Project name (defaults to current directory name).
    ///
    /// Must start with a letter or underscore and contain only
    /// alphanumeric characters, underscores, or hyphens.
    /// Cannot be a reserved Inference keyword.
    pub name: Option<String>,
}

/// Executes the `init` command.
///
/// Initializes the current directory as an Inference project by creating
/// the manifest file and optionally a source file.
///
/// # Errors
///
/// Returns an error if:
/// - The project name is invalid
/// - An Inference.toml already exists
/// - File creation fails
pub fn execute(args: &InitArgs) -> Result<()> {
    let name = args.name.as_deref();

    init_project(None, name, true)?;

    let display_name = name.map_or_else(|| String::from("current directory"), String::from);

    println!("Initialized Inference project in {display_name}");
    println!();
    println!("Next steps:");
    println!("  infs build src/main.inf --codegen -o");
    println!();
    println!("To learn more about Inference, visit:");
    println!("  https://inference-lang.org");

    Ok(())
}
