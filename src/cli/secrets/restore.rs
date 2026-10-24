use super::Selection;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct RestoreArgs {
    #[command(flatten)]
    pub selection: Selection,
    /// Password-encrypted archive created by stool secrets backup
    #[arg(long)]
    pub input: PathBuf,
    /// Target user directory (defaults to the current user's home)
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Read the password from a private external file
    #[arg(long)]
    pub passphrase_file: Option<PathBuf>,
}
