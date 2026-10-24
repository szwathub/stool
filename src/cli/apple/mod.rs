pub mod brew;
mod init;

pub use init::InitArgs;

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Manage macOS preferences and Homebrew independently of dotfiles")]
pub struct AppleCommand {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Set the current user's Dock minimization animation to suck
    Init(InitArgs),
    /// Initialize Homebrew and install software from a Brewfile
    Brew(brew::BrewCommand),
}
