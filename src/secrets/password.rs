//! Offline-backup password input and password-file validation.
use super::paths::{checked, expand, outside_git, read};
use crate::common::{fs as filesystem, terminal};
use age::secrecy::{ExposeSecret, SecretString, SecretVec};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

pub(super) fn password(path: Option<&Path>, confirm: bool) -> Result<SecretString> {
    let password = if let Some(path) = path {
        let path = expand(path)?;
        let pipe = confirm && (path.starts_with("/dev/fd") || path.starts_with("/proc/self/fd"));
        let bytes = if pipe {
            SecretVec::new(fs::read(&path).context("Read password pipe")?)
        } else {
            let path = checked(&path)?;
            outside_git(&path)?;
            let metadata = fs::metadata(&path)?;
            if !metadata.is_file() || filesystem::path_mode(&path, &metadata)? != 0o600 {
                bail!("Password file must be private (0600)");
            }
            read(&path)?
        };
        let mut text =
            std::str::from_utf8(bytes.expose_secret()).context("Backup password must be UTF-8")?;
        if let Some(stripped) = text.strip_suffix('\n') {
            text = stripped.strip_suffix('\r').unwrap_or(stripped);
        }
        SecretString::new(text.to_owned())
    } else {
        let password = terminal::prompt_passphrase("Backup password: ")
            .context("Read backup password privately from a terminal, or use --passphrase-file")?;
        if confirm
            && terminal::prompt_passphrase("Confirm backup password: ")?.expose_secret()
                != password.expose_secret()
        {
            bail!("Backup passwords did not match");
        }
        password
    };
    if password.expose_secret().is_empty() || password.expose_secret().contains(['\r', '\n', '\0'])
    {
        bail!("Backup password must be nonempty and contain no line breaks or NUL bytes");
    }
    Ok(password)
}
