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
//!
//! ## Default Behavior
//!
//! When no phase flags are given, `infs build` defaults to full compilation and
//! writes the WASM binary to disk — equivalent to `--codegen -o`. This matches
//! conventional compiler UX (e.g. `gcc foo.c`).
//!
//! ```bash
//! infs build example.inf       # parse → codegen → write out/example.wasm
//! infs build example.inf -v    # parse → codegen → write out/example.wasm + out/example.v
//! ```
//!
//! Supplying any explicit phase flag overrides the default:
//!
//! ```bash
//! infs build example.inf --parse    # parse only, no output files
//! infs build example.inf --analyze  # parse + analyze only, no output files
//! ```

use anyhow::{Context, Result, bail};
use clap::Args;
use std::path::PathBuf;
use std::process::Command;

use crate::errors::InfsError;
use crate::toolchain::find_infc;

/// Arguments for the build command.
///
/// When no phase flags are given, the command defaults to full compilation and
/// writes the WASM binary to disk — equivalent to `--codegen -o`.
///
/// ## Phase Dependencies
///
/// - `--parse`: Standalone, builds the typed AST. Overrides the default.
/// - `--analyze`: Requires parsing (automatically runs parse phase). Overrides the default.
/// - `--codegen`: Requires analysis (automatically runs parse and analyze phases).
///
/// ## Output Flags
///
/// - `-o`: Generate WASM binary file in `out/` directory
/// - `-v`: Generate Rocq (.v) translation in `out/` directory. When used without
///   explicit phase flags, implies full pipeline and also sets `-o`.
#[derive(Args, Clone)]
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

/// Applies default phase normalization to build arguments.
///
/// When no phase flag (`--parse`, `--analyze`, `--codegen`) is given, defaults
/// to full pipeline + WASM output — equivalent to `--codegen -o`.
pub(crate) fn normalize_build_args(args: &mut BuildArgs) {
    if !args.parse && !args.analyze && !args.codegen {
        args.codegen = true;
        args.generate_wasm_output = true;
    }
}

/// Executes the build command with the given arguments.
///
/// ## Execution Flow
///
/// 1. Validates that the source file exists
/// 2. Applies default normalization: no phase flags → full pipeline + WASM output
/// 3. Locates the infc compiler binary
/// 4. Builds and executes the infc command with appropriate flags
/// 5. Propagates exit code from infc
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - infc compiler cannot be found
/// - infc exits with non-zero code (as `InfsError::ProcessExitCode`)
pub fn execute(args: &BuildArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("Path not found: {}", args.path.display());
    }

    let mut args = args.clone();
    normalize_build_args(&mut args);

    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_args(parse: bool, analyze: bool, codegen: bool) -> BuildArgs {
        BuildArgs {
            path: PathBuf::from("test.inf"),
            parse,
            analyze,
            codegen,
            generate_wasm_output: false,
            generate_v_output: false,
        }
    }

    #[test]
    fn normalize_sets_full_pipeline_when_no_flags() {
        let mut args = make_args(false, false, false);
        normalize_build_args(&mut args);
        assert!(args.codegen);
        assert!(args.generate_wasm_output);
        assert!(!args.generate_v_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_parse() {
        let mut args = make_args(true, false, false);
        normalize_build_args(&mut args);
        assert!(!args.codegen);
        assert!(!args.generate_wasm_output);
    }

    #[test]
    fn normalize_does_not_override_explicit_analyze() {
        let mut args = make_args(false, true, false);
        normalize_build_args(&mut args);
        assert!(!args.codegen);
    }

    #[test]
    fn normalize_does_not_override_explicit_codegen() {
        let mut args = make_args(false, false, true);
        normalize_build_args(&mut args);
        assert!(args.codegen);
        assert!(!args.generate_wasm_output);
    }
}
