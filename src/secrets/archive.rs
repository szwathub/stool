//! Versioned offline archive, authenticated encryption and payload validation.
use super::{gpg, paths::relative, ssh, Files, Manifest, LIMIT};
use crate::common::fs as filesystem;
use age::secrecy::{ExposeSecret, SecretString, SecretVec, Zeroize};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::io::{Read, Write};

#[derive(Default)]
pub(super) struct Sensitive(Vec<u8>);
impl Sensitive {
    pub(super) fn finish(mut self) -> SecretVec<u8> {
        SecretVec::new(std::mem::take(&mut self.0))
    }
}
impl Drop for Sensitive {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl Write for Sensitive {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > LIMIT as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Key backup exceeds the size limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn add(files: &mut Files, name: String, bytes: SecretVec<u8>) -> Result<()> {
    relative(&name)?;
    let folded = name.to_ascii_lowercase();
    if files.keys().any(|old| {
        let old = old.to_ascii_lowercase();
        old == folded
            || old.starts_with(&format!("{folded}/"))
            || folded.starts_with(&format!("{old}/"))
    }) {
        bail!("Duplicate or conflicting key archive path");
    }
    if files
        .values()
        .map(|v| v.expose_secret().len())
        .sum::<usize>()
        .saturating_add(bytes.expose_secret().len())
        > LIMIT as usize
    {
        bail!("Key backup exceeds the size limit");
    }
    files.insert(name, bytes);
    Ok(())
}

pub(super) fn pack(manifest: &Manifest, files: &Files) -> Result<SecretVec<u8>> {
    let mut builder = tar::Builder::new(Sensitive::default());
    let metadata = SecretVec::new(serde_json::to_vec(manifest)?);
    for (name, bytes) in std::iter::once(("manifest.json", metadata.expose_secret().as_slice()))
        .chain(
            files
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.expose_secret().as_slice())),
        )
    {
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append_data(&mut header, name, bytes)?;
    }
    Ok(builder.into_inner()?.finish())
}
pub(super) fn unpack(payload: &[u8]) -> Result<(Manifest, Files)> {
    let mut files = Files::new();
    let mut manifest = None;
    let mut names = BTreeSet::new();
    let mut archive = tar::Archive::new(payload);
    for entry in archive.entries()? {
        let entry = entry?;
        if !entry.header().entry_type().is_file() || entry.size() > LIMIT || files.len() > 4096 {
            bail!("Key archive contains an unsupported entry");
        }
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        relative(&name)?;
        if !names.insert(name.to_ascii_lowercase()) {
            bail!("Key archive contains duplicate paths");
        }
        let mut bytes = Sensitive::default();
        std::io::copy(&mut entry.take(LIMIT + 1), &mut bytes)?;
        let bytes = bytes.finish();
        if name == "manifest.json" {
            manifest = Some(serde_json::from_slice::<Manifest>(bytes.expose_secret())?);
        } else {
            add(&mut files, name, bytes)?;
        }
    }
    let manifest = manifest.context("Key archive manifest is missing")?;
    if manifest.format != "stool-key-backup" || manifest.version != 1 {
        bail!("Unsupported key backup format or version");
    }
    if files.len() != manifest.files.len()
        || files.iter().any(|(name, bytes)| {
            manifest.files.get(name) != Some(&filesystem::digest(bytes.expose_secret()))
        })
    {
        bail!("Key backup checksum or inventory mismatch");
    }
    let mut expected = BTreeSet::new();
    if let Some(gpg) = &manifest.gpg {
        expected.extend(gpg::paths(gpg)?);
    }
    let mut ssh_names = BTreeSet::new();
    for key in &manifest.ssh {
        if !ssh_names.insert(key.path.to_ascii_lowercase()) {
            bail!("Duplicate SSH identity in manifest");
        }
        relative(&key.path)?;
        expected.insert(format!("ssh/{}", key.path));
        expected.insert(format!("ssh/{}.pub", key.path));
        ssh::validate(key, &files)?;
    }
    if expected.len() != files.len() || !expected.iter().eq(files.keys()) {
        bail!("Key backup contains missing or unexpected files");
    }
    if manifest.gpg.is_none() && manifest.ssh.is_empty() {
        bail!("Key backup is empty");
    }
    Ok((manifest, files))
}
pub(super) fn encrypt(password: &SecretString, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut writer =
        age::Encryptor::with_user_passphrase(password.clone()).wrap_output(&mut output)?;
    writer.write_all(bytes)?;
    writer.finish()?;
    Ok(output)
}
pub(super) fn decrypt(password: &SecretString, bytes: &[u8]) -> Result<SecretVec<u8>> {
    let decryptor = age::Decryptor::new(bytes).context("Invalid encrypted key backup")?;
    let age::Decryptor::Passphrase(decryptor) = decryptor else {
        bail!("Only password-encrypted offline backups are supported");
    };
    let mut reader = decryptor
        .decrypt(password, Some(20))
        .context("Unable to decrypt key backup; check the password")?
        .take(LIMIT + 1);
    let mut output = Sensitive::default();
    std::io::copy(&mut reader, &mut output).context("Invalid or damaged encrypted key backup")?;
    Ok(output.finish())
}

#[cfg(test)]
#[path = "../../tests/unit/secrets/archive.rs"]
mod tests;
