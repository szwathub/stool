//! Export selected local identities into a verified encrypted offline backup.
use super::{
    archive::{decrypt, encrypt, pack, unpack},
    gpg,
    password::password,
    paths::{checked, home, outside_git},
    report_warnings, ssh, Files, Manifest, LIMIT,
};
use crate::cli::secrets::BackupArgs;
use crate::common::fs as filesystem;
use age::secrecy::ExposeSecret;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn run(args: &BackupArgs) -> Result<()> {
    let source = if let Some(path) = &args.input {
        checked(path)?
    } else {
        home()?
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let output = checked(
        args.output
            .as_deref()
            .unwrap_or(&PathBuf::from(format!("stool-keys-{now}.tar.age"))),
    )?;
    outside_git(&output)?;
    if fs::symlink_metadata(&output).is_ok() {
        bail!("Backup output already exists; choose a new file");
    }
    let mut files = Files::new();
    let mut warnings = Vec::new();
    let gpg_home = if args.input.is_none() {
        std::env::var_os("GNUPGHOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| source.join(".gnupg"))
    } else {
        source.join(".gnupg")
    };
    let gpg = if args.selection.gpg() {
        gpg::export(&checked(&gpg_home)?, &mut files, &mut warnings)?
    } else {
        None
    };
    let ssh = if args.selection.ssh() {
        ssh::export(&source, args.input.is_none(), &mut files, &mut warnings)?
    } else {
        Vec::new()
    };
    if (args.selection.gpg && gpg.is_none())
        || (args.selection.ssh && ssh.is_empty())
        || (gpg.is_none() && ssh.is_empty())
    {
        bail!("No exportable keys found for the selected categories; no backup was written");
    }
    if args.selection.gpg() && gpg.is_none() {
        warnings.push("No exportable local GPG keys found".into());
    }
    if args.selection.ssh() && ssh.is_empty() {
        warnings.push("No local SSH private keys found".into());
    }
    let manifest = Manifest {
        // Keep the wire format stable when renaming the CLI.
        format: "stool-key-backup".into(),
        version: 1,
        created_unix: now,
        gpg,
        ssh,
        files: files
            .iter()
            .map(|(name, bytes)| (name.clone(), filesystem::digest(bytes.expose_secret())))
            .collect(),
        warnings,
    };
    let payload = pack(&manifest, &files)?;
    let password = password(args.passphrase_file.as_deref(), true)?;
    let ciphertext = encrypt(&password, payload.expose_secret())?;
    // Age framing adds overhead beyond the plaintext tar. Never publish an
    // archive that the bounded input reader would reject during restoration.
    if ciphertext.len() as u64 > LIMIT {
        bail!("Encrypted key backup exceeds the 64 MiB size limit; no backup was written");
    }
    let recovered = decrypt(&password, &ciphertext)?;
    unpack(recovered.expose_secret())?;
    let parent = output.parent().context("Backup file has no parent")?;
    checked(parent)?;
    outside_git(parent)?;
    if !parent.exists() {
        filesystem::private_dir(parent)?;
    }
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    filesystem::set_mode(staged.path(), 0o600)?;
    staged.write_all(&ciphertext)?;
    staged.as_file().sync_all()?;
    staged
        .persist_noclobber(&output)
        .map_err(|e| e.error)
        .context("Save new key backup without overwriting an existing file")?;
    report_warnings(&manifest);
    println!(
        "Encrypted key backup created successfully: {}",
        output.display()
    );
    println!(
        "GPG primary keys: {}; SSH private keys: {}",
        manifest.gpg.as_ref().map_or(0, |v| v.groups.len()),
        manifest.ssh.len()
    );
    Ok(())
}
