//! Validation shared by saved defaults and repository operations.
use anyhow::{bail, Context, Result};

pub fn validate_url(url: &str) -> Result<()> {
    if url.chars().any(char::is_control) {
        bail!("Git URL contains control characters");
    }
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("ssh://"))
    {
        let (authority, path) = rest
            .split_once('/')
            .context("Git URL needs a host and repository path")?;
        if authority.is_empty() || path.is_empty() {
            bail!("Git URL needs a host and repository path");
        }
        if url.starts_with("https://") && authority.contains('@') {
            bail!("Use a Git credential helper instead of embedding credentials in the URL");
        }
        if let Some((user, _)) = authority.rsplit_once('@') {
            if user.contains(':') {
                bail!("Do not embed passwords in the Git URL");
            }
        }
        return Ok(());
    }
    if let Some(path) = url.strip_prefix("file://") {
        if path.starts_with('/') {
            return Ok(());
        }
    }
    if let Some((host, path)) = url.split_once(':') {
        if let Some((user, hostname)) = host.split_once('@') {
            if !user.is_empty()
                && !hostname.is_empty()
                && !path.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
                && !host.starts_with('-')
            {
                return Ok(());
            }
        }
    }
    bail!("Expected an HTTPS, SSH, git@host:path, or absolute file:// Git URL");
}
