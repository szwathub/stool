use super::process;
use super::storage::EncryptionSettings;
use crate::common::fs as filesystem;
use age::secrecy::ExposeSecret;
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

pub fn suffix(settings: &EncryptionSettings) -> Result<&'static str> {
    match settings.backend.as_str() {
        "age" => Ok("age"),
        "gpg" => Ok("asc"),
        _ => bail!("Encryption backend must be age or gpg"),
    }
}

pub fn encrypt(settings: &EncryptionSettings, bytes: &[u8]) -> Result<Vec<u8>> {
    match settings.backend.as_str() {
        "age" => {
            let recipients: Vec<Box<dyn age::Recipient + Send>> = settings
                .recipients
                .iter()
                .map(|text| {
                    text.parse::<age::x25519::Recipient>()
                        .map(|key| Box::new(key) as Box<dyn age::Recipient + Send>)
                        .map_err(|_| anyhow::anyhow!("Invalid age recipient"))
                })
                .collect::<Result<_>>()?;
            let encryptor = age::Encryptor::with_recipients(recipients)
                .context("Configure encryption.recipients before encrypting")?;
            let mut output = Vec::new();
            let armor = age::armor::ArmoredWriter::wrap_output(
                &mut output,
                age::armor::Format::AsciiArmor,
            )?;
            let mut writer = encryptor.wrap_output(armor)?;
            writer.write_all(bytes)?;
            writer.finish()?.finish()?;
            Ok(output)
        }
        "gpg" => {
            let recipient = settings
                .gpg_recipient
                .as_ref()
                .context("Configure encryption.gpg_recipient")?;
            let mut command = Command::new("gpg");
            command.args([
                "--batch",
                "--yes",
                "--armor",
                "--encrypt",
                "--recipient",
                recipient,
            ]);
            let output = process::capture(command, Some(bytes))?;
            if !output.status.success() {
                bail!(
                    "gpg encryption failed ({}); check the recipient and keyring",
                    output.status
                );
            }
            Ok(output.stdout)
        }
        _ => bail!("Encryption backend must be age or gpg"),
    }
}

pub fn decrypt(
    settings: &EncryptionSettings,
    home: &Path,
    bytes: &[u8],
    extension: &str,
) -> Result<Vec<u8>> {
    if extension == "asc" || (extension != "age" && settings.backend == "gpg") {
        let mut command = Command::new("gpg");
        command.args(["--batch", "--yes", "--decrypt"]);
        let output = process::capture(command, Some(bytes))?;
        if !output.status.success() {
            bail!(
                "gpg decryption failed ({}); check your private key and agent",
                output.status
            );
        }
        return Ok(output.stdout);
    }
    let mut identities = Vec::<age::x25519::Identity>::new();
    for path in &settings.identities {
        let path = filesystem::expand_home(path, home);
        for line in std::fs::read_to_string(&path)
            .with_context(|| format!("Read age identity {}", path.display()))?
            .lines()
        {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            identities.push(
                line.parse()
                    .map_err(|_| anyhow::anyhow!("Invalid age identity in {}", path.display()))?,
            );
        }
    }
    let reader = age::armor::ArmoredReader::new(bytes);
    let decryptor = age::Decryptor::new(reader).context("Invalid age ciphertext")?;
    let mut output = Vec::new();
    match decryptor {
        age::Decryptor::Recipients(decryptor) => {
            if identities.is_empty() {
                bail!("Configure encryption.identities before decrypting age files");
            }
            decryptor
                .decrypt(identities.iter().map(|key| key as &dyn age::Identity))?
                .read_to_end(&mut output)?;
        }
        age::Decryptor::Passphrase(decryptor) => {
            let passphrase = std::env::var("STOOL_AGE_PASSPHRASE")
                .context("Set STOOL_AGE_PASSPHRASE for passphrase-encrypted age files")?;
            decryptor
                .decrypt(&age::secrecy::Secret::new(passphrase), None)?
                .read_to_end(&mut output)?;
        }
    }
    Ok(output)
}

pub fn keygen(output: &Path) -> Result<String> {
    if output.exists() {
        bail!("Identity file already exists: {}", output.display());
    }
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let identity = age::x25519::Identity::generate();
    let public = identity.to_public().to_string();
    filesystem::atomic_write(
        output,
        format!(
            "# public key: {public}\n{}\n",
            identity.to_string().expose_secret()
        )
        .as_bytes(),
        0o600,
    )?;
    Ok(public)
}

#[cfg(test)]
#[path = "../../tests/unit/dotfile/crypto.rs"]
mod tests;
