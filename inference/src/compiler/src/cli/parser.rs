use clap::Parser;

#[derive(Parser)]
pub(crate) struct Cli {
    pub(crate) path: std::path::PathBuf,
    #[clap(short = 'r', long = "rust", required = false)]
    pub(crate) rust: bool,
    #[clap(short = 'w', long = "wasm", required = false)]
    pub(crate) wasm: bool,
}
