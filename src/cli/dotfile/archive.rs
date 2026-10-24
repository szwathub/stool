use clap::Args;
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum ArchiveFormat {
    Tar,
    Tgz,
    Zip,
}
#[derive(Args, Debug)]
pub struct ArchiveArgs {
    #[arg(long, value_enum, default_value = "tar")]
    pub format: ArchiveFormat,
    #[arg(short, long)]
    pub output: PathBuf,
}
