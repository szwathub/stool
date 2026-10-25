//! Defaults shared by top-level initialization and dotfile initialization.
use crate::common::{fs as filesystem, git::validate_url, ssh};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Config {
    pub path: PathBuf,
    value: toml::Value,
}
impl Config {
    pub fn load(home: &Path) -> Result<Self> {
        let path = home.join(".stool/config.toml");
        for item in [path.parent().unwrap(), path.as_path()] {
            if fs::symlink_metadata(item).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!(
                    "Global configuration cannot be a symlink: {}",
                    item.display()
                );
            }
        }
        let value: toml::Value = match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).context("Invalid global stool config.toml")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                toml::Value::Table(Default::default())
            }
            Err(error) => return Err(error).context("Read global stool config.toml"),
        };
        if let Some(version) = value.get("version") {
            if version.as_integer() != Some(1) {
                bail!("Unsupported global stool configuration version");
            }
        }
        let config = Self { path, value };
        config.dotfile_url()?;
        config.dotfile_ssh_key()?;
        Ok(config)
    }
    pub fn dotfile_url(&self) -> Result<Option<&str>> {
        let Some(dotfile) = self.value.get("dotfile") else {
            return Ok(None);
        };
        let table = dotfile
            .as_table()
            .context("Global dotfile configuration must be a table")?;
        let Some(url) = table.get("url") else {
            return Ok(None);
        };
        let url = url
            .as_str()
            .context("Global dotfile.url must be a string")?;
        validate_url(url)?;
        Ok(Some(url))
    }
    pub fn dotfile_ssh_key(&self) -> Result<Option<&Path>> {
        let Some(dotfile) = self.value.get("dotfile") else {
            return Ok(None);
        };
        let table = dotfile
            .as_table()
            .context("Global dotfile configuration must be a table")?;
        let Some(key) = table.get("ssh_key") else {
            return Ok(None);
        };
        let key = key
            .as_str()
            .context("Global dotfile.ssh_key must be a string")?;
        if key.is_empty() || key.chars().any(char::is_control) {
            bail!("Global dotfile.ssh_key must be a nonempty path without control characters");
        }
        let path = Path::new(key);
        if !path.is_absolute() {
            bail!("Global dotfile.ssh_key must be an absolute path");
        }
        Ok(Some(path))
    }
    pub fn save_dotfile(home: &Path, url: &str, ssh_key: Option<&Path>) -> Result<()> {
        validate_url(url)?;
        let ssh_key = ssh_key
            .map(|key| ssh::identity_path(home, key))
            .transpose()?;
        let root = home.join(".stool");
        // Serialize read/merge/write so unrelated defaults survive concurrent changes.
        Self::load(home)?;
        let _lock = filesystem::lock(&root, true)?;
        let mut config = Self::load(home)?;
        let same_url = config.dotfile_url()? == Some(url);
        let table = config
            .value
            .as_table_mut()
            .context("Global configuration must be a table")?;
        table.insert("version".into(), 1.into());
        let dotfile = table
            .entry("dotfile")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        let dotfile = dotfile
            .as_table_mut()
            .context("Global dotfile configuration must be a table")?;
        dotfile.insert("url".into(), url.into());
        if let Some(key) = &ssh_key {
            dotfile.insert(
                "ssh_key".into(),
                key.to_str()
                    .context("SSH identity path must be UTF-8")?
                    .into(),
            );
        } else if !same_url {
            dotfile.remove("ssh_key");
        }
        filesystem::atomic_write(
            &config.path,
            toml::to_string_pretty(&config.value)?.as_bytes(),
            0o600,
        )
    }
}

pub fn home() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .context("Cannot determine user home directory")?;
    filesystem::resolve(Path::new(&home))
}
