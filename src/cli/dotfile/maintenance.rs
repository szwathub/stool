use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct AddArgs {
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,
    #[arg(long)]
    pub template: bool,
    #[arg(long)]
    pub encrypt: bool,
}
#[derive(Args, Debug)]
pub struct EditArgs {
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,
    #[arg(long)]
    pub apply: bool,
}
