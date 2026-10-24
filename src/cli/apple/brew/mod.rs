mod init;
mod install;

pub use init::InitArgs;
pub use install::InstallArgs;

use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(about = "Initialize Homebrew and explicitly install a Brewfile")]
pub struct BrewCommand {
    /// Select a Homebrew prefix instead of HOMEBREW_PREFIX, PATH or the native default
    #[arg(long, global = true, value_name = "DIR")]
    pub prefix: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Reuse Homebrew or install it with the official installer
    Init(InitArgs),
    /// Ensure Homebrew is available, then install a trusted Brewfile without routine upgrades
    Install(InstallArgs),
}

#[derive(Args, Debug)]
pub struct Options {
    /// Show the plan without downloading, installing or evaluating the Brewfile
    #[arg(long)]
    pub dry_run: bool,
    /// Run without terminal input (requires privileges to be prepared separately)
    #[arg(long)]
    pub non_interactive: bool,
}
