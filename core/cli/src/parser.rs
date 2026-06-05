//! Command line argument parsing for the Inference compiler.
//!
//! This module defines the CLI interface using `clap`. The `Cli` struct captures
//! all command line flags and arguments passed to the `infc` binary.
//!
//! For comprehensive usage documentation, see `README.md` in this crate.

use clap::{Parser, ValueEnum};

/// Compilation mode selected via the `--mode` flag.
///
/// Mirrors [`inference_wasm_codegen::CompilationMode`] so the CLI surface can derive
/// `clap::ValueEnum` without forcing that trait onto the codegen crate's type.
///
/// - `Compile`: strips spec functions, applies release optimizations to produce a
///   production-style WASM binary. This is the resolved default when neither
///   `--mode` nor `-v` was passed.
/// - `Proof`: keeps spec functions unoptimized so the Rocq translation preserves
///   structural correspondence with the source. Implies `-v` after normalization,
///   because the `.v` artifact is the proof-mode deliverable.
///
/// The `Cli::mode` field is `Option<CliMode>` so the absence of `--mode` is
/// distinguishable from `--mode compile`; this lets `-v` alone auto-promote to
/// `Proof` while `--mode compile -v` keeps compile semantics.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliMode {
    Compile,
    Proof,
}

impl From<CliMode> for inference_wasm_codegen::CompilationMode {
    fn from(mode: CliMode) -> Self {
        match mode {
            CliMode::Compile => inference_wasm_codegen::CompilationMode::Compile,
            CliMode::Proof => inference_wasm_codegen::CompilationMode::Proof,
        }
    }
}

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
/// - `-o`: Generate WASM binary file in the output directory
/// - `-v`: Generate Rocq (.v) translation in the output directory; when used
///   without any explicit phase flag, implies full pipeline + `-o`
/// - `--out-dir <path>`: Override the output directory (default `out/`, relative
///   to the current working directory). Applies to both `.wasm` and `.v`.
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
/// Full compilation with Rocq translation (implies `--mode proof`):
/// ```bash
/// infc example.inf -v
/// ```
///
/// Proof-mode compilation (keeps spec functions, implies `-v`):
/// ```bash
/// infc example.inf --mode proof
/// ```
///
/// V output from compile-mode WASM (specs stripped — escape hatch):
/// ```bash
/// infc example.inf --mode compile -v
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

    /// Directory for output artifacts (`.wasm` and `.v`).
    ///
    /// When omitted, artifacts are written to `out/` relative to the current
    /// working directory, preserving the historical behavior. When supplied,
    /// both the `.wasm` and the `.v` (if requested) land under this directory
    /// instead. The directory is created automatically if it does not exist.
    ///
    /// This is pure output plumbing: `infc` gains no project awareness from it.
    /// `infs` uses it in project mode to honor `[verification] output-dir`.
    #[clap(long = "out-dir")]
    pub(crate) out_dir: Option<std::path::PathBuf>,

    /// Run the parse phase to build the typed AST.
    ///
    /// This phase reads the source file, runs the custom parser, and constructs
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
    /// When used without an explicit `--mode`, implies `--mode proof` — the `.v`
    /// is only meaningful with spec functions preserved, and `-v` alone against a
    /// spec-stripped (compile-mode) WASM produces a near-empty `.v`. Pass
    /// `--mode compile -v` explicitly to opt back into that behavior.
    ///
    /// This enables formal verification of the compiled program using the
    /// Rocq proof assistant.
    #[clap(short = 'v', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_v_output: bool,

    /// Compilation mode.
    ///
    /// - `compile`: production WASM with specs stripped and release optimizations
    ///   applied. The resolved default when neither `--mode` nor `-v` is supplied.
    /// - `proof`: preserves spec functions unoptimized so the Rocq translation
    ///   maintains 1:1 structural correspondence with the source. Implies `-v`
    ///   because the `.v` artifact is the proof-mode deliverable.
    ///
    /// When `--mode` is omitted, `-v` promotes the effective mode to `proof`; if
    /// `-v` is also absent the effective mode is `compile`.
    #[clap(long = "mode", value_enum)]
    pub(crate) mode: Option<CliMode>,

    /// Directory to search for external `.wasm` modules referenced by
    /// `use { … } from <module>;`.
    ///
    /// Repeatable; directories are searched in the order given, ahead of any
    /// `INFERENCE_WASM_LIB_PATH` environment directories. A logical module
    /// `a::b` resolves to `<dir>/a/b.wasm` under each directory.
    #[clap(short = 'L', long = "wasm-lib-dir", value_name = "DIR")]
    pub(crate) wasm_lib_dirs: Vec<std::path::PathBuf>,

    /// A manifest-declared external `.wasm` module, as `<name>=<path>`.
    ///
    /// Repeatable; binds the logical module `<name>` directly to the `.wasm`
    /// file at `<path>`, taking precedence over every `-L` / `INFERENCE_*`
    /// search directory. `infs build` forwards one entry per
    /// `Inference.toml [wasm-dependencies]` declaration; direct `infc` callers
    /// may pass them by hand.
    #[clap(long = "wasm-dep", value_name = "NAME=PATH")]
    pub(crate) wasm_deps: Vec<String>,

    /// Print the git commit hash embedded at build time and exit 0.
    ///
    /// Used by `infs build` to detect version drift between paired `infs` and
    /// `infc` binaries. Does not require a source file argument.
    #[clap(long = "commit-hash", action = clap::ArgAction::SetTrue)]
    pub(crate) commit_hash: bool,

    /// Print the compiler ABI version (`<major>.<minor>`) and exit 0.
    ///
    /// Used by `infs build` to verify that the invoked `infc` speaks a CLI/IO
    /// contract it understands. Does not require a source file argument.
    #[clap(long = "abi-version", action = clap::ArgAction::SetTrue)]
    pub(crate) abi_version: bool,
}
