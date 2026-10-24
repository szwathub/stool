use crate::common::constants::ROOT_DIR;
use crate::common::fs::{atomic_write, digest, private_dir, resolve};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub repo: String,
    pub branch: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub recurse_submodules: bool,
    #[serde(default = "empty_object")]
    pub data: serde_json::Value,
    #[serde(default)]
    pub encryption: EncryptionSettings,
    #[serde(default)]
    pub git: GitSettings,
    #[serde(default)]
    pub editor: Option<String>,
}

pub fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct EncryptionSettings {
    pub backend: String,
    pub recipients: Vec<String>,
    pub identities: Vec<PathBuf>,
    pub gpg_recipient: Option<String>,
}
impl Default for EncryptionSettings {
    fn default() -> Self {
        Self {
            backend: "age".into(),
            recipients: Vec::new(),
            identities: Vec::new(),
            gpg_recipient: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GitSettings {
    pub builtin: bool,
    pub lfs: bool,
}

pub struct Paths {
    pub home: PathBuf,
    pub root: PathBuf,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub state_file: PathBuf,
    pub config: Option<Config>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fingerprint {
    File { hash: String, mode: u32 },
    Directory { mode: u32 },
    Symlink { target: PathBuf },
}

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub entries: BTreeMap<String, Fingerprint>,
}

impl Paths {
    pub fn load(source: Option<&Path>, destination: Option<&Path>) -> Result<Self> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("Cannot determine user home directory")?;
        let home = resolve(Path::new(&home))?;
        let root = resolve(&home.join(ROOT_DIR).join("dotfile"))?;
        let config: Option<Config> = match fs::read_to_string(root.join("config.toml")) {
            Ok(text) => {
                let cfg: Config = toml::from_str(&text).context("Invalid dotfile config.toml")?;
                if cfg.version != 1 {
                    bail!("Unsupported dotfile configuration version {}", cfg.version);
                }
                if !cfg.data.is_object() {
                    bail!("Configuration data must be an object");
                }
                Some(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).context("Read dotfile configuration"),
        };
        let default_source = root.join("source");
        let source = resolve(source.unwrap_or_else(|| {
            config
                .as_ref()
                .map(|c| c.source.as_path())
                .unwrap_or(&default_source)
        }))?;
        let destination = resolve(destination.unwrap_or_else(|| {
            config
                .as_ref()
                .map(|c| c.destination.as_path())
                .unwrap_or(&home)
        }))?;
        if destination.starts_with(&source) || root.starts_with(&source) {
            bail!("Source must not contain the destination or stool control directory");
        }
        if destination.starts_with(&root) {
            bail!("Destination must not be inside the stool control directory");
        }
        let key = digest(&serde_json::to_vec(&(&source, &destination))?);
        Ok(Self {
            home,
            state_file: root.join("state").join(format!("{key}.json")),
            root,
            source,
            destination,
            config,
        })
    }

    pub fn state(&self) -> Result<State> {
        match fs::read(&self.state_file) {
            Ok(data) => serde_json::from_slice(&data).context("Invalid dotfile state"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e).context("Read dotfile state"),
        }
    }

    pub fn save_state(&self, state: &State) -> Result<()> {
        private_dir(self.state_file.parent().unwrap())?;
        atomic_write(&self.state_file, &serde_json::to_vec_pretty(state)?, 0o600)
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        atomic_write(
            &self.root.join("config.toml"),
            toml::to_string_pretty(config)?.as_bytes(),
            0o600,
        )
    }

    pub fn check_target(&self, target: &Path) -> Result<()> {
        let global_control = self.home.join(ROOT_DIR);
        let reserved_control = target
            .strip_prefix(&self.home)
            .ok()
            .and_then(|relative| relative.components().next())
            .is_some_and(|part| {
                part.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(ROOT_DIR)
            });
        if !target.starts_with(&self.destination)
            || target == self.destination
            || target.starts_with(&self.source)
            || self.source.starts_with(target)
            || target.starts_with(&self.root)
            || self.root.starts_with(target)
            || reserved_control
            || global_control.starts_with(target)
        {
            bail!(
                "Target overlaps source/control storage or escapes destination: {}",
                target.display()
            );
        }
        Ok(())
    }

    pub fn encryption(&self) -> EncryptionSettings {
        self.config
            .as_ref()
            .map(|cfg| cfg.encryption.clone())
            .unwrap_or_default()
    }
}

pub fn relative_path(raw: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for part in raw.components() {
        match part {
            Component::Normal(name) => {
                #[cfg(windows)]
                {
                    let text = name.to_str().context("Path must be UTF-8")?;
                    let base = text.split('.').next().unwrap_or("").to_ascii_uppercase();
                    if text.contains([':', '<', '>', '"', '|', '?', '*'])
                        || text.ends_with(['.', ' '])
                        || ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
                        || (base.len() == 4
                            && (base.starts_with("COM") || base.starts_with("LPT"))
                            && matches!(base.as_bytes()[3], b'1'..=b'9'))
                    {
                        bail!("Invalid Windows destination name: {text}");
                    }
                }
                result.push(name)
            }
            Component::CurDir => {}
            _ => bail!("Expected a path inside the destination: {}", raw.display()),
        }
    }
    if result.as_os_str().is_empty() {
        bail!("A nonempty relative path is required");
    }
    Ok(result)
}
