//! Process-scoped SSH identities without changing the user's agent or configuration.
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn resolve_identity_path(home: &Path, key: &Path) -> Result<PathBuf> {
    let path = filesystem::expand_home(key, home);
    filesystem::resolve(&path)
}

pub fn identity_path(home: &Path, key: &Path) -> Result<PathBuf> {
    let path = resolve_identity_path(home, key)?;
    if !fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        bail!("SSH private key is not a regular file: {}", path.display());
    }
    if !private_key(&path)? {
        bail!(
            "Expected an SSH private key, not a public key or configuration: {}",
            path.display()
        );
    }
    config_path(&path)?;
    Ok(path)
}

fn private_key(path: &Path) -> Result<bool> {
    let mut header = [0; 80];
    let count = fs::File::open(path)
        .with_context(|| format!("Read SSH identity {}", path.display()))?
        .read(&mut header)?;
    Ok([
        b"-----BEGIN OPENSSH PRIVATE KEY-----".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN DSA PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
        b"-----BEGIN PRIVATE KEY-----",
        b"-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ]
    .iter()
    .any(|prefix| header[..count].starts_with(prefix)))
}

pub fn candidates(home: &Path) -> Result<Vec<PathBuf>> {
    fn walk(directory: &Path, keys: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(&entry.path(), keys)?;
            } else if kind.is_file() && private_key(&entry.path())? {
                keys.push(entry.path());
            }
        }
        Ok(())
    }
    let directory = home.join(".ssh");
    let mut keys = Vec::new();
    if fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.is_dir()) {
        walk(&directory, &mut keys).context("Find local SSH private keys")?;
    }
    keys.sort();
    Ok(keys)
}

pub fn host(url: &str) -> Result<Option<String>> {
    let parsed = gix::url::parse(url.into()).context("Parse Git SSH URL")?;
    if parsed.scheme != gix::url::Scheme::Ssh {
        return Ok(None);
    }
    let host = parsed.host().context("SSH URL needs a host")?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-:[]".contains(c))
    {
        bail!("Invalid SSH host");
    }
    Ok(Some(host.to_owned()))
}

#[derive(Clone)]
pub struct Identity {
    pub key: PathBuf,
    pub home: PathBuf,
    pub host: String,
}
impl Identity {
    pub fn new(home: &Path, url: &str, key: &Path) -> Result<Option<Self>> {
        let Some(host) = host(url)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            key: identity_path(home, key)?,
            home: home.to_owned(),
            host,
        }))
    }

    pub fn session(&self, batch: bool) -> Result<Session> {
        let mut file = tempfile::Builder::new().prefix("stool-ssh-").tempfile()?;
        writeln!(
            file,
            "Host {}\n  IdentitiesOnly yes\n  IdentityFile {}",
            self.host,
            config_path(&self.key)?
        )?;
        let user = self.home.join(".ssh/config");
        if user.is_file() {
            writeln!(file, "Host *\n  Include {}", config_path(&user)?)?;
        }
        #[cfg(unix)]
        let system = PathBuf::from("/etc/ssh/ssh_config");
        #[cfg(windows)]
        let system = PathBuf::from(
            std::env::var_os("PROGRAMDATA").unwrap_or_else(|| "C:/ProgramData".into()),
        )
        .join("ssh/ssh_config");
        #[cfg(not(any(unix, windows)))]
        let system = PathBuf::from("/etc/ssh/ssh_config");
        if system.is_file() {
            writeln!(file, "Host *\n  Include {}", config_path(&system)?)?;
        }
        file.flush()?;
        let command = format!(
            "ssh -F {}{}",
            shell_words::quote(&command_path(file.path())?),
            if batch {
                " -o BatchMode=yes -o ConnectTimeout=10"
            } else {
                ""
            }
        );
        Ok(Session {
            command,
            _config: file,
        })
    }
}

pub struct Session {
    pub command: String,
    _config: tempfile::NamedTempFile,
}
impl Session {
    pub fn apply(&self, command: &mut Command) {
        command
            .env("GIT_SSH_COMMAND", &self.command)
            .env("GIT_SSH_VARIANT", "ssh");
    }
}

fn command_path(path: &Path) -> Result<String> {
    #[cfg(windows)]
    let path = dunce::simplified(path);
    let text = path.to_str().context("SSH identity path must be UTF-8")?;
    if text.chars().any(char::is_control) || text.contains("${") {
        bail!("SSH paths cannot contain control characters or environment expansions");
    }
    #[cfg(windows)]
    let text = text.replace('\\', "/");
    Ok(text.to_owned())
}
fn config_path(path: &Path) -> Result<String> {
    let text = command_path(path)?;
    // SSH expands percent tokens itself; shell quoting alone does not prevent that.
    Ok(format!(
        "\"{}\"",
        text.replace('%', "%%")
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/common_ssh.rs"]
mod tests;
