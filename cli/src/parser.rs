use clap::Parser;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Inference compiler CLI (infc)",
    long_about = "The 'infc' command runs one or more compilation phases over a single .inf source file. \
Parse builds the typed AST; analyze performs semantic/type inference; codegen emits WASM and can translate to V when -o is supplied."
)]
#[allow(clippy::struct_excessive_bools)] // CLI flag aggregation; bools are fine here.
pub(crate) struct Cli {
    // Path to the source file (later should support pointing to the project file infp)
    pub(crate) path: std::path::PathBuf,
    // Runs source parsing and AST building, exit with 0 if successful
    #[clap(long = "parse", action = clap::ArgAction::SetTrue)]
    pub(crate) parse: bool,
    // Runs type checking, exit with 0 if successful
    #[clap(long = "analyze", action = clap::ArgAction::SetTrue)]
    pub(crate) analyze: bool,
    // Runs code generation, exit with 0 if successful
    #[clap(long = "codegen", action = clap::ArgAction::SetTrue)]
    pub(crate) codegen: bool,
    // Generates output .v files
    #[clap(short = 'o', action = clap::ArgAction::SetTrue)]
    pub(crate) generate_v_output: bool,
}
