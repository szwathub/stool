use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Git URL (defaults to dotfile.url saved by stool init)
    pub repo: Option<String>,
    /// SSH private key (otherwise use saved identity, SSH configuration or discovered keys)
    #[arg(long, value_name = "FILE")]
    pub ssh_key: Option<PathBuf>,
    #[arg(long)]
    pub branch: Option<String>,
    /// Deploy after cloning
    #[arg(long)]
    pub apply: bool,
    /// Explicitly enable recursive submodules (the default for new repositories)
    #[arg(long, conflicts_with = "no_recurse_submodules")]
    pub recurse_submodules: bool,
    /// Disable recursive submodule initialization and subsequent updates
    #[arg(long, conflicts_with = "recurse_submodules")]
    pub no_recurse_submodules: bool,
    /// Clone using the built-in Git backend
    #[arg(long)]
    pub builtin: bool,
}
