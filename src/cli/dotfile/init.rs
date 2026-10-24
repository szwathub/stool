use clap::Args;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Git URL (defaults to dotfile.url saved by stool init)
    pub repo: Option<String>,
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
