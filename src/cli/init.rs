use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(
    about = "Save dotfiles defaults, initialize Apple settings and optionally restore offline keys"
)]
pub struct InitCommand {
    /// Default Git URL; saved without cloning or contacting the repository
    #[arg(long, value_name = "URL")]
    pub dotfile_url: Option<String>,
    /// Password-encrypted offline archive produced by stool secrets backup
    #[arg(long, value_name = "FILE")]
    pub key_file: Option<PathBuf>,
    /// Trusted local Brewfile (defaults to ~/.Brewfile; missing default is skipped)
    #[arg(long, value_name = "FILE")]
    pub brewfile: Option<PathBuf>,
    /// Preview without saving configuration, decrypting keys or changing the system
    #[arg(long)]
    pub dry_run: bool,
    /// Allow package installation without terminal input; prepare privileges separately
    #[arg(long)]
    pub non_interactive: bool,
    /// Private external backup password file (requires --key-file)
    #[arg(long, requires = "key_file", value_name = "FILE")]
    pub passphrase_file: Option<PathBuf>,
}
