use super::Selection;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct BackupArgs {
    #[command(flatten)]
    pub selection: Selection,
    /// Source user directory (defaults to the current user environment)
    #[arg(long)]
    pub input: Option<PathBuf>,
    /// New backup file (defaults to stool-keys-<timestamp>.tar.age in the current directory)
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Read the password from a private external file or inherited pipe
    #[arg(long)]
    pub passphrase_file: Option<PathBuf>,
}
