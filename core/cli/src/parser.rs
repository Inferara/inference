//! Command line argument parsing for the Inference compiler.
//!
//! This module defines the CLI interface using `clap`. The `Cli` struct captures
//! all command line flags and arguments passed to the `infc` binary.

use clap::Parser;

/// Command line interface definition for the Inference compiler.
///
/// The `infc` compiler operates in phases, and users must explicitly request
/// which phases to run via command line flags. Phases execute in canonical order
/// (parse → analyze → codegen) regardless of flag order.
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
///
/// Output flags only take effect when `--codegen` is specified.
///
/// ## Examples
///
/// Parse only:
/// ```bash
/// infc example.inf --parse
/// ```
///
/// Full compilation with WASM output:
/// ```bash
/// infc example.inf --codegen -o
/// ```
///
/// Full compilation with Rocq translation:
/// ```bash
/// infc example.inf --codegen -o -v
/// ```
#[derive(Parser)]
#[command(
    name = "infc",
    author,
    version,
    about = "Inference compiler CLI (infc)",
    long_about = "The 'infc' command runs one or more compilation phases over a single .inf source file. \
Parse builds the typed AST; analyze performs semantic/type inference; codegen emits WASM and can translate to V when -o is supplied."
)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    /// Path to the source file to compile.
    ///
    /// Currently only single-file compilation is supported. Multi-file projects
    /// and project file (`.infp`) support is planned for future releases.
    pub(crate) path: std::path::PathBuf,

    /// Run the parse phase to build the typed AST.
    ///
    /// This phase reads the source file, runs tree-sitter parsing, and constructs
    /// an arena-allocated typed AST. If parsing succeeds, the compiler prints
    /// "Parsed: <filepath>" and exits with code 0.
    ///
    /// Parse errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "parse", action = clap::ArgAction::SetTrue)]
    pub(crate) parse: bool,

    /// Run the analyze phase for semantic and type inference.
    ///
    /// This phase performs type checking and semantic validation on the AST.
    /// The parse phase is automatically run first if not already requested.
    ///
    /// Analysis errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "analyze", action = clap::ArgAction::SetTrue)]
    pub(crate) analyze: bool,

    /// Run the codegen phase to emit WebAssembly binary.
    ///
    /// This phase generates LLVM IR and compiles it to WebAssembly. Both parse
    /// and analyze phases are automatically run first if not already requested.
    ///
    /// Use `-o` to write the WASM binary to disk, and `-v` to additionally
    /// generate a Rocq translation.
    ///
    /// Codegen errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "codegen", action = clap::ArgAction::SetTrue)]
    pub(crate) codegen: bool,

    /// Generate output WASM binary file.
    ///
    /// When specified with `--codegen`, writes the compiled WebAssembly binary
    /// to `out/<source_name>.wasm` relative to the current working directory.
    ///
    /// This flag has no effect without `--codegen`.
    #[clap(short = 'o', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_wasm_output: bool,

    /// Generate Rocq (.v) translation file.
    ///
    /// When specified with `--codegen`, translates the compiled WebAssembly
    /// to Rocq (Coq) format and writes it to `out/<source_name>.v` relative
    /// to the current working directory.
    ///
    /// This enables formal verification of the compiled program using the
    /// Rocq proof assistant.
    ///
    /// This flag has no effect without `--codegen`.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_v_output: bool,
}
