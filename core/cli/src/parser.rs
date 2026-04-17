//! Command line argument parsing for the Inference compiler.
//!
//! This module defines the CLI interface using `clap`. The `Cli` struct captures
//! all command line flags and arguments passed to the `infc` binary.
//!
//! For comprehensive usage documentation, see `README.md` in this crate.

use clap::Parser;

/// Command line interface definition for the Inference compiler.
///
/// ## Default Behavior
///
/// When no phase flags are given, `infc` defaults to full compilation and writes
/// the WASM binary to disk — equivalent to `--codegen -o`. Supplying any explicit
/// phase flag (`--parse`, `--analyze`, `--codegen`) overrides this default and
/// runs only the requested phases.
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
/// - `-v`: Generate Rocq (.v) translation in `out/` directory; when used without
///   any explicit phase flag, implies full pipeline + `-o`
///
/// Output flags only take effect when `--codegen` is active (explicitly or via default).
///
/// ## Examples
///
/// Full compilation (default — no flags required):
/// ```bash
/// infc example.inf
/// ```
///
/// Full compilation with Rocq translation:
/// ```bash
/// infc example.inf -v
/// ```
///
/// Parse only (overrides default):
/// ```bash
/// infc example.inf --parse
/// ```
///
/// Explicit full compilation (equivalent to the default):
/// ```bash
/// infc example.inf --codegen -o
/// ```
#[derive(Parser)]
#[command(
    name = "infc",
    author,
    version,
    about = "Inference compiler CLI (infc)",
    long_about = "The 'infc' command compiles a single .inf source file. \
By default (no flags), it runs the full pipeline and writes out/<name>.wasm. \
Use --parse, --analyze, or --codegen to run only specific phases. \
Add -v to also produce a Rocq (.v) translation for formal verification."
)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    /// Path to the source file to compile.
    ///
    /// Currently only single-file compilation is supported. Multi-file projects
    /// and project file (`.infp`) support is planned for future releases.
    ///
    /// Optional so informational flags (e.g. `--commit-hash`) can run without
    /// a source file argument. Regular compilation still requires a path and
    /// exits with an error if one is not supplied.
    pub(crate) path: Option<std::path::PathBuf>,

    /// Run the parse phase to build the typed AST.
    ///
    /// This phase reads the source file, runs tree-sitter parsing, and constructs
    /// an arena-allocated typed AST. If parsing succeeds, the compiler prints
    /// "Parsed: <filepath>" and exits with code 0.
    ///
    /// Overrides the default full-pipeline behavior: supplying this flag means
    /// only the parse phase runs (no codegen, no output files).
    ///
    /// Parse errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "parse", action = clap::ArgAction::SetTrue)]
    pub(crate) parse: bool,

    /// Run the analyze phase for semantic and type inference.
    ///
    /// This phase performs type checking and semantic validation on the AST.
    /// The parse phase is automatically run first if not already requested.
    ///
    /// Overrides the default full-pipeline behavior: supplying this flag means
    /// only parse + analyze run (no codegen, no output files).
    ///
    /// Analysis errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "analyze", action = clap::ArgAction::SetTrue)]
    pub(crate) analyze: bool,

    /// Run the codegen phase to emit WebAssembly binary.
    ///
    /// This phase generates WebAssembly binary from the typed AST. Both parse
    /// and analyze phases are automatically run first if not already requested.
    ///
    /// When used without `-o` or `-v`, codegen runs but no output files are written.
    /// Use `-o` to write the WASM binary to disk, and `-v` to additionally
    /// generate a Rocq translation.
    ///
    /// Codegen errors will be reported to stderr and the process exits with code 1.
    #[clap(long = "codegen", action = clap::ArgAction::SetTrue)]
    pub(crate) codegen: bool,

    /// Generate output WASM binary file.
    ///
    /// Writes the compiled WebAssembly binary to `out/<source_name>.wasm`
    /// relative to the current working directory. Requires codegen to be active
    /// (either via `--codegen` or the default full-pipeline behavior).
    ///
    /// Set automatically when no phase flags are given (default behavior).
    #[clap(short = 'o', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_wasm_output: bool,

    /// Generate Rocq (.v) translation file.
    ///
    /// Translates the compiled WebAssembly to Rocq (Coq) format and writes it
    /// to `out/<source_name>.v` relative to the current working directory.
    ///
    /// When used without any explicit phase flag, implies full pipeline + `-o`:
    /// `infc file.inf -v` produces both `out/file.wasm` and `out/file.v`.
    ///
    /// This enables formal verification of the compiled program using the
    /// Rocq proof assistant.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_v_output: bool,

    /// Print the git commit hash embedded at build time and exit 0.
    ///
    /// Used by `infs build` to detect version drift between paired `infs` and
    /// `infc` binaries. Does not require a source file argument.
    #[clap(long = "commit-hash", action = clap::ArgAction::SetTrue)]
    pub(crate) commit_hash: bool,
}
