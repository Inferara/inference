#![warn(clippy::pedantic)]

//! Build command for the infs CLI.
//!
//! Compiles Inference source files through the three-phase compilation pipeline:
//! parse, analyze, and codegen. This module contains logic migrated from the
//! legacy `infc` CLI with improved error handling using `anyhow::Context`.
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
use inference::{analyze, codegen, parse, type_check, wasm_to_v};
use std::{fs, path::{Path, PathBuf}};

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
/// 3. Executes compilation phases in canonical order
/// 4. Generates output files if requested
///
/// ## Errors
///
/// Returns an error if:
/// - The source file does not exist
/// - No phase flags are specified
/// - Any compilation phase fails
/// - Output file writing fails
pub fn execute(args: &BuildArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("Path not found: {}", args.path.display());
    }

    let output_path = PathBuf::from("out");
    let need_parse = args.parse;
    let need_analyze = args.analyze;
    let need_codegen = args.codegen;

    if !(need_parse || need_analyze || need_codegen) {
        bail!("At least one of --parse, --analyze, or --codegen must be specified");
    }

    let source_code = fs::read_to_string(&args.path)
        .with_context(|| format!("Failed to read source file: {}", args.path.display()))?;

    let arena = parse(source_code.as_str())
        .with_context(|| format!("Parse error in {}", args.path.display()))?;
    println!("Parsed: {}", args.path.display());

    if !need_codegen && !need_analyze {
        return Ok(());
    }

    let typed_context = type_check(arena)
        .with_context(|| format!("Type checking failed for {}", args.path.display()))?;

    analyze(&typed_context)
        .with_context(|| format!("Analysis failed for {}", args.path.display()))?;
    println!("Analyzed: {}", args.path.display());

    if !need_codegen {
        return Ok(());
    }

    let wasm = codegen(&typed_context)
        .with_context(|| format!("Codegen failed for {}", args.path.display()))?;
    println!("WASM generated");

    let source_fname = args
        .path
        .file_stem()
        .unwrap_or_else(|| std::ffi::OsStr::new("module"))
        .to_str()
        .unwrap_or("module");

    if args.generate_wasm_output {
        write_wasm_output(&output_path, source_fname, &wasm)?;
    }

    if args.generate_v_output {
        write_v_output(&output_path, source_fname, &wasm)?;
    }

    Ok(())
}

/// Writes the WASM binary to the output directory.
fn write_wasm_output(output_path: &Path, source_fname: &str, wasm: &[u8]) -> Result<()> {
    fs::create_dir_all(output_path)
        .with_context(|| format!("Failed to create output directory: {}", output_path.display()))?;

    let wasm_file_path = output_path.join(format!("{source_fname}.wasm"));
    fs::write(&wasm_file_path, wasm)
        .with_context(|| format!("Failed to write WASM file: {}", wasm_file_path.display()))?;

    println!("WASM generated at: {}", wasm_file_path.display());
    Ok(())
}

/// Writes the Rocq (.v) translation to the output directory.
fn write_v_output(output_path: &Path, source_fname: &str, wasm: &[u8]) -> Result<()> {
    let wasm_vec = wasm.to_vec();
    let v_output = wasm_to_v(source_fname, &wasm_vec)
        .with_context(|| format!("WASM to Rocq translation failed for {source_fname}"))?;

    fs::create_dir_all(output_path)
        .with_context(|| format!("Failed to create output directory: {}", output_path.display()))?;

    let v_file_path = output_path.join(format!("{source_fname}.v"));
    fs::write(&v_file_path, v_output)
        .with_context(|| format!("Failed to write Rocq file: {}", v_file_path.display()))?;

    println!("V generated at: {}", v_file_path.display());
    Ok(())
}
