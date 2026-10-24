use super::Options;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Trusted Brewfile to install (defaults to ~/.Brewfile)
    #[arg(long, value_name = "FILE")]
    pub file: Option<PathBuf>,
    #[command(flatten)]
    pub options: Options,
}
