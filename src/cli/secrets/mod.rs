mod backup;
mod restore;
mod selection;

pub use backup::BackupArgs;
pub use restore::RestoreArgs;
pub use selection::Selection;

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
#[command(about = "Back up and restore GPG and SSH keys independently of dotfiles")]
pub struct SecretsCommand {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create one password-encrypted offline key backup outside Git workspaces
    Backup(BackupArgs),
    /// Validate and manually restore an offline key backup
    Restore(RestoreArgs),
}
