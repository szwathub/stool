use clap::Args;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Show current preferences and planned changes without writing
    #[arg(long)]
    pub dry_run: bool,
}
