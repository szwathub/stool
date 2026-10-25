//! Command and argument definitions; execution lives in the feature modules.

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod apple;
pub mod dotfile;
pub mod init;
pub mod secrets;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "stool",
    about = "Developer environment, dotfiles and offline key backups",
    version,
    disable_help_subcommand = false,
    color = clap::ColorChoice::Auto,
    args_conflicts_with_subcommands = false
)]
pub struct Definition<C: clap::Subcommand> {
    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::SetTrue,
        help = "Show more debugging information"
    )]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: C,
}

pub type Cli = Definition<Commands>;

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(flatten)]
    Base(BaseCommands),
    #[cfg(target_os = "macos")]
    Apple(apple::AppleCommand),
}

#[derive(Subcommand, Debug)]
pub enum BaseCommands {
    Init(init::InitCommand),
    Dotfile(dotfile::DotfileCommand),
    Secrets(secrets::SecretsCommand),
    /// Generate shell completions from the current command definitions
    Completions {
        shell: clap_complete::Shell,
    },
    /// Render the manual, or write all command manuals to a directory
    Man {
        #[arg(long)]
        output: Option<std::path::PathBuf>,
    },
}
