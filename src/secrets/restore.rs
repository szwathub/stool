//! Target preflight and reusable staged restoration of validated key archives.
use super::{
    archive::{decrypt, unpack},
    gpg,
    password::password,
    paths::{checked, home, outside_git, read, relative},
    report_warnings, Files, Manifest,
};
use crate::cli::secrets::RestoreArgs;
use crate::common::fs as filesystem;
use age::secrecy::ExposeSecret;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn target_files(
    manifest: &Manifest,
    files: &Files,
    gpg: bool,
    ssh: bool,
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    if gpg {
        for name in files
            .keys()
            .filter(|name| name.starts_with("gpg/revocations/"))
        {
            result.insert(
                name.clone(),
                format!(
                    ".gnupg/openpgp-revocs.d/{}",
                    name.trim_start_matches("gpg/revocations/")
                ),
            );
        }
    }
    if ssh {
        for key in &manifest.ssh {
            for suffix in ["", ".pub"] {
                result.insert(
                    format!("ssh/{}{suffix}", key.path),
                    format!(".ssh/{}{suffix}", key.path),
                );
            }
        }
    }
    result
}
fn preflight(target: &Path, names: &BTreeMap<String, String>, files: &Files) -> Result<()> {
    checked(target)?;
    outside_git(target)?;
    for (source, name) in names {
        let path = checked(&target.join(relative(name)?))?;
        outside_git(&path)?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.is_file() || read(&path)?.expose_secret() != files[source].expose_secret()
            {
                bail!(
                    "Existing key conflicts with backup: {}; no files were overwritten",
                    path.display()
                );
            }
        }
    }
    Ok(())
}
pub(crate) struct RestoreSession {
    target: PathBuf,
    manifest: Manifest,
    files: Files,
}
impl RestoreSession {
    pub(crate) fn check_file(input: &Path) -> Result<()> {
        read(&checked(input)?)?;
        Ok(())
    }
    pub(crate) fn open(args: &RestoreArgs) -> Result<Self> {
        let input = checked(&args.input)?;
        let target = if let Some(path) = &args.output {
            checked(path)?
        } else {
            home()?
        };
        outside_git(&target)?;
        let encrypted = read(&input)?;
        let password = password(args.passphrase_file.as_deref(), false)?;
        let payload = decrypt(&password, encrypted.expose_secret())?;
        let (manifest, files) = unpack(payload.expose_secret())?;
        Ok(Self {
            target,
            manifest,
            files,
        })
    }
    pub(crate) fn has_gpg(&self) -> bool {
        self.manifest.gpg.is_some()
    }
    pub(crate) fn has_ssh(&self) -> bool {
        !self.manifest.ssh.is_empty()
    }
    pub(crate) fn preflight(&self) -> Result<()> {
        preflight(
            &self.target,
            &target_files(&self.manifest, &self.files, self.has_gpg(), self.has_ssh()),
            &self.files,
        )?;
        if self.has_gpg() {
            gpg::preflight(&self.target)?;
        }
        Ok(())
    }
    pub(crate) fn restore(&self, do_gpg: bool, do_ssh: bool) -> Result<()> {
        let (manifest, files, target) = (&self.manifest, &self.files, &self.target);
        let do_gpg = do_gpg && self.has_gpg();
        let do_ssh = do_ssh && self.has_ssh();
        if !do_gpg && !do_ssh {
            bail!("Backup contains no keys of the selected category");
        }
        let names = target_files(manifest, files, do_gpg, do_ssh);
        preflight(target, &names, files)?;
        if do_gpg {
            gpg::preflight(target)?;
            gpg::validate(manifest.gpg.as_ref().unwrap(), files)?;
        }
        if do_gpg {
            gpg::restore(target,manifest.gpg.as_ref().unwrap(),files)
            .context("GPG restore failed and may have imported some keys; retry after fixing the cause; this restore phase did not write SSH keys; earlier completed phases are retained")?;
            println!(
                "Restored GPG keys and ownertrust ({} primary keys).",
                manifest.gpg.as_ref().unwrap().groups.len()
            );
        }
        for (source, name) in &names {
            // Check again immediately before each write; identical files are retained.
            let path = checked(&target.join(relative(name)?))?;
            outside_git(&path)?;
            let category = if name.starts_with(".ssh/") {
                "SSH"
            } else {
                "GPG revocation"
            };
            let mut parents = Vec::new();
            let mut parent = path.parent().unwrap();
            while parent.starts_with(target) && parent != target {
                parents.push(parent.to_owned());
                parent = parent.parent().unwrap();
            }
            if !target.exists() {
                filesystem::private_dir(target)?;
            }
            for directory in parents.into_iter().rev() {
                checked(&directory)?;
                filesystem::private_dir(&directory)?;
            }
            if fs::symlink_metadata(&path).is_ok() {
                if read(&path)?.expose_secret() != files[source].expose_secret() {
                    bail!(
                    "{category} key changed during restore: {}; retry after resolving the conflict",
                    path.display()
                );
                }
                filesystem::set_mode(&path, 0o600)?;
                continue;
            }
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options
                .open(&path)
                .with_context(|| format!("Create restored {category} key without replacement"))?;
            let result = (|| -> Result<()> {
                filesystem::set_mode(&path, 0o600)?;
                output.write_all(files[source].expose_secret())?;
                output.sync_all()?;
                Ok(())
            })();
            if let Err(error) = result {
                drop(output);
                let _ = fs::remove_file(&path);
                return Err(error).with_context(||format!("Save restored {category} key; completed imports/files are retained and can be retried"));
            }
        }
        if do_ssh {
            println!("Restored SSH key pairs: {}", manifest.ssh.len());
        }
        report_warnings(manifest);
        println!("Key restore completed: {}", target.display());
        Ok(())
    }
}

pub(super) fn run(args: &RestoreArgs) -> Result<()> {
    RestoreSession::open(args)?.restore(args.selection.gpg(), args.selection.ssh())
}
