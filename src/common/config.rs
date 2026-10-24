//! Defaults shared by top-level initialization and dotfile initialization.
use crate::common::{fs as filesystem, git::validate_url};
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
    pub fn save_url(home: &Path, url: &str) -> Result<()> {
        validate_url(url)?;
        let root = home.join(".stool");
        // Serialize read/merge/write so unrelated defaults survive concurrent changes.
        Self::load(home)?;
        let _lock = filesystem::lock(&root, true)?;
        let mut config = Self::load(home)?;
        let table = config
            .value
            .as_table_mut()
            .context("Global configuration must be a table")?;
        table.insert("version".into(), 1.into());
        let dotfile = table
            .entry("dotfile")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        dotfile
            .as_table_mut()
            .context("Global dotfile configuration must be a table")?
            .insert("url".into(), url.into());
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
