//! Key-specific path guards and bounded secret reads.
use super::{archive::Sensitive, LIMIT};
use crate::common::fs as filesystem;
use age::secrecy::SecretVec;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub(super) fn home() -> Result<PathBuf> {
    let path = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("Cannot determine user home directory")?;
    checked(Path::new(&path))
}
pub(super) fn expand(path: &Path) -> Result<PathBuf> {
    if path == Path::new("~") {
        return home();
    }
    if let Ok(relative) = path.strip_prefix("~/") {
        return Ok(home()?.join(relative));
    }
    Ok(path.to_owned())
}

/// Inspect the original spelling before canonicalization, including every existing parent.
pub(super) fn checked(path: &Path) -> Result<PathBuf> {
    let path = expand_absolute(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => bail!("Parent traversal is not allowed in key paths"),
            Component::CurDir => continue,
            Component::Prefix(_) => {
                // A drive/UNC prefix is not a filesystem entry until its root is appended.
                current.push(component.as_os_str());
                continue;
            }
            _ => current.push(component.as_os_str()),
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("Key path cannot contain a symlink: {}", current.display())
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e).context("Inspect key path"),
        }
    }
    filesystem::resolve(&path)
}
fn expand_absolute(path: &Path) -> Result<PathBuf> {
    // Do not call home() from here: its path uses this same validation.
    let path = if path == Path::new("~") || path.starts_with("~/") {
        let root = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("Cannot determine user home directory")?;
        if path == Path::new("~") {
            PathBuf::from(root)
        } else {
            PathBuf::from(root).join(path.strip_prefix("~/")?)
        }
    } else {
        path.to_owned()
    };
    Ok(if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    })
}
pub(super) fn outside_git(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        if parent.join(".git").exists()
            || fs::symlink_metadata(parent.join(".git")).is_ok()
            || (parent.join("HEAD").is_file()
                && parent.join("objects").is_dir()
                && parent.join("refs").is_dir())
        {
            bail!(
                "Keep key backups and restored keys outside Git workspaces: {}",
                path.display()
            );
        }
    }
    Ok(())
}
pub(super) fn relative(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.contains(['\\', ':', '\0']) || value.starts_with('/') {
        bail!("Invalid key archive path");
    }
    for part in value.split('/') {
        let upper = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.eq_ignore_ascii_case(".git")
            || part.ends_with(['.', ' '])
            || ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
            || (upper.len() == 4
                && (upper.starts_with("COM") || upper.starts_with("LPT"))
                && matches!(upper.as_bytes()[3], b'1'..=b'9'))
        {
            bail!("Invalid key archive path");
        }
    }
    Ok(PathBuf::from(value))
}
pub(super) fn read(path: &Path) -> Result<SecretVec<u8>> {
    let path = checked(path)?;
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        bail!("Key input must be a bounded regular file");
    }
    let mut bytes = Sensitive::default();
    let mut file = fs::File::open(&path)?.take(LIMIT + 1);
    std::io::copy(&mut file, &mut bytes)?;
    Ok(bytes.finish())
}

#[cfg(all(test, windows))]
#[path = "../../tests/unit/secrets/paths_windows.rs"]
mod windows_tests;
