use clap::Parser;

#[derive(Parser)]
pub(crate) struct Cli {
    pub(crate) path: std::path::PathBuf,
    #[clap(short = 'g', long = "generate", required = false)]
    pub(crate) out: Option<String>,
    #[clap(short = 'o', long = "output", required = false)]
    pub(crate) output: Option<String>,
}
