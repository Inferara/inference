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
//! - `run` - Build and execute WASM: under wasmtime, or in process under the
//!   `SpaceWasm` flight interpreter for a `spacewasm` build
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
//! for interactive project management.
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
//! infs build example.inf
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

mod artifact;
mod commands;
mod errors;
mod project;
#[cfg(test)]
mod testing;
mod toolchain;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{
    build, component, default, doctor, init, install, list, new, run, self_cmd, uninstall, version,
    versions,
};
use errors::InfsError;

/// Inference unified CLI toolchain.
///
/// The `infs` command provides access to the complete Inference toolchain
/// including compilation, analysis, and project management features.
#[derive(Parser)]
#[command(
    name = "infs",
    author,
    version,
    about = "Inference Start is a unified CLI toolchain",
    long_about = "The 'infs' command is the unified entry point for the Inference programming \
    language toolchain. Use subcommands like 'build' to compile source files.",
    after_help = "\
COMPILER RESOLUTION:
    The infc compiler is located using the following priority order:
    1. INFC_PATH environment variable (explicit override)
    2. System PATH (via 'which infc')
    3. Managed toolchain (~/.inference/toolchains/VERSION/infc)

ENVIRONMENT VARIABLES:
    INFS_NO_TUI             Disable interactive TUI
    INFC_PATH               Explicit path to infc binary
    INFERENCE_HOME          Toolchain directory (default: ~/.inference)
    INFS_DIST_SERVER        Distribution server URL (default: https://inference-lang.org)"
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
    /// With a path, compiles that .inf file; without one, finds Inference.toml
    /// by walking up from the current directory and compiles the project's
    /// src/main.inf. Either way every file reached through use imports is
    /// compiled too. Each build runs all phases (parse, analyze, codegen) and
    /// writes the WASM binary; -v also writes a Rocq (.v) translation.
    Build(build::BuildArgs),

    /// Build and run an Inference program.
    // The long help is built by `run::long_about`, which lists the F´ reference
    // hosts a `spacewasm` build may import from the runner's own table, so the
    // list `--help` prints is the list the interpreter registers.
    #[command(long_about = run::long_about())]
    Run(run::RunArgs),

    /// Display version information.
    ///
    /// Shows the version of the infs CLI. Use -v or --verbose for detailed
    /// information including build date, platform, and compiler version.
    Version(version::VersionArgs),

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

    /// List available toolchain versions.
    ///
    /// Fetches the release manifest and displays all available versions
    /// with their stability status and platform availability.
    Versions(versions::VersionsArgs),

    /// Set the default toolchain version.
    ///
    /// Changes the default toolchain used for compilation.
    Default(default::DefaultArgs),

    /// Check installation health.
    ///
    /// Verifies that all required components are installed and configured
    /// correctly. Reports any issues with suggested remediation steps.
    Doctor,

    /// Manage optional toolchain components.
    ///
    /// Installs, lists, or removes managed components such as `wasm-opt`
    /// (Binaryen), the optimizer used by the `[build.wasm-opt]` manifest table.
    Component(component::ComponentArgs),

    /// Manage the infs binary itself.
    ///
    /// Provides subcommands for updating or managing the infs CLI tool.
    #[command(name = "self")]
    SelfCmd(self_cmd::SelfArgs),
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        let exit_code = handle_error(&e);
        std::process::exit(exit_code);
    }
}

/// Handles an error and returns the appropriate exit code.
///
/// For `ProcessExitCode` errors, returns the embedded exit code without
/// printing an error message (the subprocess already printed its output).
/// For all other errors, prints the error and returns exit code 1.
fn handle_error(e: &anyhow::Error) -> i32 {
    if let Some(InfsError::ProcessExitCode { code }) = e.downcast_ref::<InfsError>() {
        return *code;
    }
    eprintln!("Error: {e:?}");
    1
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::New(args)) => new::execute(&args),
        Some(Commands::Init(args)) => init::execute(&args),
        Some(Commands::Build(args)) => build::execute(&args),
        Some(Commands::Run(args)) => run::execute(&args),
        Some(Commands::Version(args)) => version::execute(&args),
        Some(Commands::Install(args)) => install::execute(&args).await,
        Some(Commands::Uninstall(args)) => uninstall::execute(&args).await,
        Some(Commands::List) => list::execute().await,
        Some(Commands::Versions(args)) => versions::execute(&args).await,
        Some(Commands::Default(args)) => default::execute(&args).await,
        Some(Commands::Doctor) => doctor::execute().await,
        Some(Commands::Component(args)) => component::execute(&args).await,
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;
    use inference_spacewasm_runner::{LIMITS_FROM, fprime};

    /// The long help of `infs run`, as `infs run --help` prints it.
    fn run_long_help() -> String {
        let mut command = Cli::command();
        command
            .find_subcommand_mut("run")
            .expect("`infs run` is a subcommand")
            .render_long_help()
            .to_string()
    }

    /// `infs run --help` lists every F´ reference host a `spacewasm` build may
    /// import, each on a line of its own exactly as the runner's table writes
    /// it, and names the interpreter release the build runs under.
    ///
    /// Fails if a row goes missing or drifts from the table the interpreter
    /// registers, which a list copied into the help would do the moment a row
    /// changed, or if the release stops being named.
    #[test]
    fn run_help_lists_every_reference_host_the_interpreter_registers() {
        let help = run_long_help();
        let rows = fprime::reference_table_lines();
        assert_eq!(rows.len(), fprime::REFERENCE_HOSTS.len());
        for row in rows {
            assert!(
                help.lines().any(|line| line == row),
                "`infs run --help` must carry the line `{row}`, got:\n{help}"
            );
        }
        assert!(
            help.contains(&format!("SpaceWasm flight interpreter\n({LIMITS_FROM})")),
            "`infs run --help` must name the interpreter release, got:\n{help}"
        );
    }

    /// The short help keeps to one line, and the long help opens with it: the
    /// table is long-help material only.
    #[test]
    fn run_short_help_is_one_line() {
        let mut command = Cli::command();
        let run = command
            .find_subcommand_mut("run")
            .expect("`infs run` is a subcommand");
        assert_eq!(
            run.get_about().map(ToString::to_string).as_deref(),
            Some("Build and run an Inference program")
        );
        assert!(run_long_help().starts_with("Build and run an Inference program.\n\n"));
    }
}
