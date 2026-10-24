use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct TemplateArgs {
    pub template: Option<String>,
    #[arg(long, conflicts_with = "template")]
    pub file: Option<PathBuf>,
    #[arg(long)]
    pub init: bool,
}
