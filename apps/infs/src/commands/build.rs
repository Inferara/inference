//! Build command for the infs CLI.
//!
//! Compiles Inference source files by delegating to the `infc` compiler
//! via subprocess. This module acts as a lightweight bootstrapper, validating
//! arguments and forwarding them to infc.
//!
//! ## Compilation Phases
//!
//! 1. **Parse** (`--parse`) - Builds the typed AST using tree-sitter
//! 2. **Analyze** (`--analyze`) - Performs type checking and semantic validation
//! 3. **Codegen** (`--codegen`) - Emits WebAssembly binary
//!
//! Phases execute in canonical order (parse -> analyze -> codegen) regardless
//! of the order flags appear on the command line. Each phase depends on the previous.

use anyhow::{Context, Result, bail};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

use crate::errors::InfsError;
use crate::toolchain::find_infc;

/// Arguments for the build command.
///
/// The build command operates in phases, and users must explicitly request
/// which phases to run via command line flags.
///
/// ## Phase Dependencies
///
/// - `--parse`: Standalone, builds the typed AST
/// - `--analyze`: Requires parsing (automatically runs parse phase)
/// - `--codegen`: Requires analysis (automatically runs parse and analyze phases)
///
/// ## Output Flags
///
/// - `-o`: Generate WASM binary file in `out/` directory
/// - `-v`: Generate Rocq (.v) translation in `out/` directory
#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct BuildArgs {
    /// Path to the source file to compile.
    pub path: PathBuf,

    /// Run the parse phase to build the typed AST.
    #[clap(long = "parse", action = clap::ArgAction::SetTrue)]
    pub parse: bool,

    /// Run the analyze phase for semantic and type inference.
    #[clap(long = "analyze", action = clap::ArgAction::SetTrue)]
    pub analyze: bool,

    /// Run the codegen phase to emit WebAssembly binary.
    #[clap(long = "codegen", action = clap::ArgAction::SetTrue)]
    pub codegen: bool,

    /// Generate output WASM binary file.
    #[clap(short = 'o', action = clap::ArgAction::SetTrue)]
    pub generate_wasm_output: bool,

    /// Generate Rocq (.v) translation file.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub generate_v_output: bool,
}

/// Executes the build command with the given arguments.
///
/// ## Execution Flow
///
/// 1. Validates that the source file exists
/// 2. Ensures at least one phase flag is specified
/// 3. Locates the infc compiler binary
/// 4. Builds and executes the infc command with appropriate flags
/// 5. Propagates exit code from infc
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - No phase flags are specified
/// - infc compiler cannot be found
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
pub fn execute(args: &BuildArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("Path not found: {}", args.path.display());
    }

    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    if !(need_parse || need_analyze || need_codegen) {
        bail!("At least one of --parse, --analyze, or --codegen must be specified");
    }

    let infc_path = find_infc()?;

    let mut cmd = Command::new(&infc_path);
    cmd.arg(&args.path);

    if need_parse {
        cmd.arg("--parse");
    }
    if need_analyze {
        cmd.arg("--analyze");
    }
    if need_codegen {
        cmd.arg("--codegen");
    }
    if args.generate_wasm_output {
        cmd.arg("-o");
    }
    if args.generate_v_output {
        cmd.arg("-v");
    }

    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute infc at {}", infc_path.display()))?;

    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        Err(InfsError::process_exit_code(code).into())
    }
}
