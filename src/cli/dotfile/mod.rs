mod apply;
mod archive;
mod init;
mod maintenance;
mod template;

pub use apply::{ApplyArgs, Prompt};
pub use archive::{ArchiveArgs, ArchiveFormat};
pub use init::InitArgs;
pub use maintenance::{AddArgs, EditArgs};
pub use template::TemplateArgs;

use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(about = "Manage dotfiles from a Git repository")]
pub struct DotfileCommand {
    /// Local source repository (saved by init)
    #[arg(long, global = true)]
    pub source: Option<PathBuf>,
    /// Destination directory (defaults to the user's home)
    #[arg(long, global = true)]
    pub destination: Option<PathBuf>,
    /// Override template data (key=value; JSON values are accepted)
    #[arg(long, global = true)]
    pub data: Vec<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Clone a Git URL and save configuration
    Init(InitArgs),
    /// Preview content and permission changes without writing
    Diff,
    /// List pending changes and conflicts
    Status,
    /// Deploy the local source repository
    Apply(ApplyArgs),
    /// Fast-forward the source repository and deploy changes
    Update,
    /// List managed destination paths
    Managed,
    /// Import existing destination files into source state
    Add(AddArgs),
    /// Edit source files (encrypted files are decrypted only in a private temporary directory)
    Edit(EditArgs),
    /// List destination files not managed by source
    Unmanaged,
    /// Return nonzero when the destination differs from source
    Verify,
    /// Diagnose repository, configuration, state and optional executables
    Doctor,
    /// Export rendered target files as tar, tar.gz, or zip
    Archive(ArchiveArgs),
    /// Evaluate a Jinja template using repository and local data
    ExecuteTemplate(TemplateArgs),
    /// Print merged template data as JSON
    Data,
    /// Generate an age identity and print its public recipient
    Keygen {
        #[arg(long)]
        output: PathBuf,
    },
    /// Start a shell in the source repository, or print its path
    Cd {
        #[arg(long)]
        print: bool,
    },
}
