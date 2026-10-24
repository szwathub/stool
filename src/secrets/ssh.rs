use super::{
    archive::add,
    paths::{checked, read, relative},
    process::capture,
    Files,
};
use age::secrecy::{ExposeSecret, SecretString, SecretVec};
use anyhow::{bail, Context, Result};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Key {
    pub path: String,
    fingerprint: String,
}
fn string<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8]> {
    if bytes.len() < 4 {
        bail!("Invalid SSH key structure");
    }
    let size = u32::from_be_bytes(bytes[..4].try_into()?) as usize;
    *bytes = &bytes[4..];
    if size > bytes.len() {
        bail!("Invalid SSH key structure");
    }
    let value = &bytes[..size];
    *bytes = &bytes[size..];
    Ok(value)
}
fn public(bytes: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("Invalid SSH public key")?;
    let mut parts = text.split_whitespace();
    let kind = parts.next().context("SSH public key type is missing")?;
    let encoded = parts.next().context("SSH public key is missing")?;
    let blob = STANDARD
        .decode(encoded)
        .context("Invalid SSH public key encoding")?;
    let mut remaining = blob.as_slice();
    if string(&mut remaining)? != kind.as_bytes() || remaining.is_empty() {
        bail!("SSH public key algorithm mismatch");
    }
    Ok(blob)
}
fn fingerprint(blob: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(blob)))
}
fn openssh_public(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let text = std::str::from_utf8(bytes).context("Invalid SSH private key encoding")?;
    if !text.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----") {
        return Ok(None);
    }
    let encoded = SecretString::new(
        text.lines()
            .skip(1)
            .take_while(|line| *line != "-----END OPENSSH PRIVATE KEY-----")
            .collect::<String>(),
    );
    if !text
        .trim_end()
        .ends_with("-----END OPENSSH PRIVATE KEY-----")
    {
        bail!("Truncated OpenSSH private key");
    }
    let decoded = SecretVec::new(
        STANDARD
            .decode(encoded.expose_secret())
            .context("Invalid OpenSSH private key encoding")?,
    );
    let mut bytes = decoded
        .expose_secret()
        .strip_prefix(b"openssh-key-v1\0")
        .context("Invalid OpenSSH key format")?;
    let cipher = string(&mut bytes)?;
    let kdf = string(&mut bytes)?;
    let options = string(&mut bytes)?;
    if cipher.is_empty()
        || kdf.is_empty()
        || (cipher == b"none" && (kdf != b"none" || !options.is_empty()))
        || bytes.len() < 4
    {
        bail!("Invalid OpenSSH key parameters");
    }
    let count = u32::from_be_bytes(bytes[..4].try_into()?);
    bytes = &bytes[4..];
    if count != 1 {
        bail!("OpenSSH key must contain one identity");
    }
    let public = string(&mut bytes)?.to_vec();
    let private = string(&mut bytes)?;
    if private.len() < 16 || !bytes.is_empty() {
        bail!("Invalid OpenSSH private key payload");
    }
    let mut structure = public.as_slice();
    let kind = string(&mut structure)?;
    if kind.is_empty() || structure.is_empty() {
        bail!("Invalid OpenSSH public key payload");
    }
    Ok(Some(public))
}
fn is_private(header: &[u8]) -> bool {
    [
        b"-----BEGIN OPENSSH PRIVATE KEY-----".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN DSA PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN PRIVATE KEY-----",
        b"-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ]
    .iter()
    .any(|prefix| header.starts_with(prefix))
}
fn walk(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<std::path::PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let meta = entry.file_type()?;
        if meta.is_symlink() {
            if entry.file_name() != "config" {
                warnings.push(format!(
                    "SSH symlink was not followed: {}",
                    path.strip_prefix(root)?.display()
                ));
            }
        } else if meta.is_dir() {
            walk(root, &path, paths, warnings)?;
        } else if meta.is_file() {
            paths.push(path);
        }
    }
    Ok(())
}

pub(super) fn export(
    home: &Path,
    inspect_agent: bool,
    files: &mut Files,
    warnings: &mut Vec<String>,
) -> Result<Vec<Key>> {
    let directory = checked(&home.join(".ssh"))?;
    let mut candidates = Vec::new();
    if directory.exists() {
        walk(&directory, &directory, &mut candidates, warnings)?;
    }
    candidates.sort();
    let mut keys = Vec::new();
    for path in candidates {
        let mut header = [0u8; 80];
        let count = fs::File::open(&path)?.read(&mut header)?;
        if !is_private(&header[..count]) {
            continue;
        }
        let name = path
            .strip_prefix(&directory)?
            .to_str()
            .context("SSH key filename must be UTF-8")?
            .replace('\\', "/");
        relative(&name)?;
        let private = read(&path)?;
        let blob = openssh_public(private.expose_secret())?;
        let public = if let Some(blob) = blob {
            let mut fields = blob.as_slice();
            let kind = std::str::from_utf8(string(&mut fields)?)?;
            SecretVec::new(format!("{kind} {}\n", STANDARD.encode(&blob)).into_bytes())
        } else {
            // Legacy PEM input is converted to a public key directly from the
            // original source file; its passphrase is handled by ssh-keygen.
            let mut derive = Command::new("ssh-keygen");
            derive.arg("-y").arg("-f").arg(&path);
            capture(derive, None, "ssh-keygen")?
        };
        let blob = public_blob(&public)?;
        let mut algorithm = blob.as_slice();
        if string(&mut algorithm)?.starts_with(b"sk-") {
            warnings.push(format!("SSH {name} requires its original security key; the local key handle alone cannot restore the hardware secret"));
        }
        let companion = path.with_file_name(format!(
            "{}.pub",
            path.file_name().unwrap().to_string_lossy()
        ));
        let public = if fs::symlink_metadata(&companion).is_ok() {
            let candidate = read(&companion)?;
            if public_blob(&candidate)? != blob {
                bail!("SSH public key does not match its private key: {name}");
            }
            candidate
        } else {
            public
        };
        add(files, format!("ssh/{name}"), private)?;
        add(files, format!("ssh/{name}.pub"), public)?;
        keys.push(Key {
            path: name,
            fingerprint: fingerprint(&blob),
        });
    }
    if inspect_agent && std::env::var_os("SSH_AUTH_SOCK").is_some() {
        match Command::new("ssh-add").arg("-L").output() {
            Ok(output) if output.status.success() => {
                for line in output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
                    let blob = public(line).context("Inspect SSH agent public identity")?;
                    let fingerprint = fingerprint(&blob);
                    if !keys.iter().any(|key| key.fingerprint == fingerprint) {
                        warnings.push(format!("SSH agent identity {fingerprint} has no private-key file under .ssh; its private key was not backed up"));
                    }
                }
            }
            Ok(output) if output.status.code() == Some(1) => (),
            _ => warnings.push("SSH agent could not be inspected; this backup includes only local private-key files under .ssh".into()),
        }
    }
    Ok(keys)
}
fn public_blob(bytes: &SecretVec<u8>) -> Result<Vec<u8>> {
    public(bytes.expose_secret())
}
pub(super) fn validate(key: &Key, files: &Files) -> Result<()> {
    let private = files
        .get(&format!("ssh/{}", key.path))
        .context("SSH private key is missing")?;
    let public = files
        .get(&format!("ssh/{}.pub", key.path))
        .context("SSH public key is missing")?;
    if !is_private(private.expose_secret()) {
        bail!("SSH private key format is invalid");
    }
    let blob = public_blob(public)?;
    if fingerprint(&blob) != key.fingerprint {
        bail!("SSH fingerprint mismatch");
    }
    if let Some(actual) = openssh_public(private.expose_secret())? {
        if actual != blob {
            bail!("SSH private/public key mismatch");
        }
    }
    Ok(())
}
