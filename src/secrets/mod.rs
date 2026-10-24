//! Offline GPG/SSH lifecycle, independent of dotfile deployment.
mod archive;
mod backup;
mod gpg;
mod password;
mod paths;
mod process;
mod restore;
mod ssh;

pub(crate) use gpg::available as gpg_available;
pub(crate) use restore::RestoreSession;

use crate::cli::secrets::{Commands, SecretsCommand};
use age::secrecy::SecretVec;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const LIMIT: u64 = 64 * 1024 * 1024;
type Files = BTreeMap<String, SecretVec<u8>>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    created_unix: u64,
    gpg: Option<gpg::Inventory>,
    ssh: Vec<ssh::Key>,
    files: BTreeMap<String, String>,
    warnings: Vec<String>,
}

pub fn run(command: &SecretsCommand) -> Result<()> {
    match &command.command {
        Commands::Backup(args) => backup::run(args),
        Commands::Restore(args) => restore::run(args),
    }
}

fn report_warnings(manifest: &Manifest) {
    for warning in &manifest.warnings {
        eprintln!("Warning: {warning}");
    }
}
