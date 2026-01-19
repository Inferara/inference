#![warn(clippy::pedantic)]

//! # Inference Unified CLI Toolchain (infs)
//!
//! The `infs` command is the unified entry point for the Inference programming
//! language toolchain. It provides subcommands for building, analyzing, and
//! managing Inference projects.
//!
//! ## Subcommands
//!
//! - `new` - Create a new Inference project
//! - `init` - Initialize an existing directory as an Inference project
//! - `build` - Compile Inference source files
//! - `version` - Display version information
//! - `install` - Install toolchain versions
//! - `uninstall` - Remove toolchain versions
//! - `list` - List installed toolchains
//! - `default` - Set default toolchain version
//! - `doctor` - Check installation health
//! - `self update` - Update infs itself
//!
//! ## Usage Modes
//!
//! ### Interactive Mode (default)
//!
//! When run without subcommands, `infs` will launch a TUI (Terminal User Interface)
//! for interactive project management. This mode is planned for Phase 4.
//!
//! ### Headless Mode (`--headless`)
//!
//! When run with `--headless` but no subcommand, `infs` displays help information
//! instead of launching the TUI.
//!
//! ## Examples
//!
//! Create a new project:
//! ```bash
//! infs new myproject
//! ```
//!
//! Build a source file:
//! ```bash
//! infs build example.inf --codegen -o
//! ```
//!
//! Install the latest toolchain:
//! ```bash
//! infs install
//! ```
//!
//! Display version:
//! ```bash
//! infs version
//! ```

mod commands;
mod project;
mod toolchain;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{build, default, doctor, init, install, list, new, self_cmd, uninstall, version};

/// Inference unified CLI toolchain.
///
/// The `infs` command provides access to the complete Inference toolchain
/// including compilation, analysis, and project management features.
#[derive(Parser)]
#[command(
    name = "infs",
    author,
    version,
    about = "Inference unified CLI toolchain",
    long_about = "The 'infs' command is the unified entry point for the Inference programming \
    language toolchain. Use subcommands like 'build' to compile source files."
)]
pub struct Cli {
    /// Run in headless mode without TUI.
    ///
    /// When specified without a subcommand, displays help information
    /// instead of launching the interactive TUI.
    #[clap(long = "headless", global = true, action = clap::ArgAction::SetTrue)]
    pub headless: bool,

    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands for the infs CLI.
#[derive(Subcommand)]
pub enum Commands {
    /// Create a new Inference project.
    ///
    /// Creates a new directory with a standard Inference project structure
    /// including Inference.toml manifest, src/main.inf entry point, and
    /// directories for tests and proofs.
    New(new::NewArgs),

    /// Initialize an existing directory as an Inference project.
    ///
    /// Creates an Inference.toml manifest and src/main.inf in the current
    /// directory without creating a new parent directory.
    Init(init::InitArgs),

    /// Compile Inference source files.
    ///
    /// The build command runs one or more compilation phases over a single
    /// .inf source file. Phases execute in canonical order: parse, analyze,
    /// codegen.
    Build(build::BuildArgs),

    /// Display version information.
    Version,

    /// Install a toolchain version.
    ///
    /// Downloads and installs a specific version of the Inference toolchain.
    /// If no version is specified, installs the latest stable version.
    Install(install::InstallArgs),

    /// Uninstall a toolchain version.
    ///
    /// Removes an installed toolchain version from the system.
    Uninstall(uninstall::UninstallArgs),

    /// List installed toolchain versions.
    ///
    /// Displays all installed toolchain versions and indicates which
    /// one is currently set as the default.
    List,

    /// Set the default toolchain version.
    ///
    /// Changes the default toolchain used for compilation.
    Default(default::DefaultArgs),

    /// Check installation health.
    ///
    /// Verifies that all required components are installed and configured
    /// correctly. Reports any issues with suggested remediation steps.
    Doctor,

    /// Manage the infs binary itself.
    ///
    /// Provides subcommands for updating or managing the infs CLI tool.
    #[command(name = "self")]
    SelfCmd(self_cmd::SelfArgs),
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {e:?}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::New(args)) => new::execute(&args),
        Some(Commands::Init(args)) => init::execute(&args),
        Some(Commands::Build(args)) => build::execute(&args),
        Some(Commands::Version) => version::execute(),
        Some(Commands::Install(args)) => install::execute(&args).await,
        Some(Commands::Uninstall(args)) => uninstall::execute(&args).await,
        Some(Commands::List) => list::execute().await,
        Some(Commands::Default(args)) => default::execute(&args).await,
        Some(Commands::Doctor) => doctor::execute().await,
        Some(Commands::SelfCmd(args)) => self_cmd::execute(&args).await,
        None => {
            if cli.headless || !tui::should_use_tui() {
                println!("infs: Inference unified CLI toolchain");
                println!();
                println!("Run 'infs --help' for usage information.");
                println!("Run 'infs build --help' for build command options.");
                Ok(())
            } else {
                tui::run()
            }
        }
    }
}
