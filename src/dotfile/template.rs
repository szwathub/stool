use super::secrets::Secrets;
use super::storage::{self, Config, Paths};
use anyhow::{bail, Context, Result};
use minijinja::value::{Rest, Value as TemplateValue};
use minijinja::{Environment, Error, ErrorKind, UndefinedBehavior};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

pub struct Engine {
    env: Environment<'static>,
    pub data: Value,
}

fn error(message: impl ToString) -> Error {
    Error::new(ErrorKind::InvalidOperation, message.to_string())
}

pub fn merge(target: &mut Value, overlay: Value) {
    match (target, overlay) {
        (Value::Object(target), Value::Object(overlay)) => {
            for (key, value) in overlay {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, value) => *target = value,
    }
}

pub fn source_root(paths: &Paths) -> Result<PathBuf> {
    let mut root = paths.source.clone();
    let path = paths.source.join(".stoolroot");
    if path.exists() {
        regular(&path)?;
        let value = fs::read_to_string(path)?;
        let value = value.trim();
        if !value.is_empty() && value != "." {
            root = paths.source.join(storage::relative_path(Path::new(value))?);
            let resolved = fs::canonicalize(&root).context("Resolve source root")?;
            if !resolved.starts_with(&paths.source) {
                bail!("Source root escapes the repository");
            }
            root = resolved;
        }
    }
    if !root.is_dir() {
        bail!("Source root is not a directory: {}", root.display());
    }
    for version_root in [&paths.source, &root] {
        let path = version_root.join(".stoolversion");
        if path.exists() {
            regular(&path)?;
            let requirement = fs::read_to_string(path)?;
            let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
            let matches = match semver::Version::parse(requirement.trim()) {
                Ok(minimum) => current >= minimum,
                Err(_) => semver::VersionReq::parse(requirement.trim())?.matches(&current),
            };
            if !matches {
                bail!(
                    "Repository requires stool {}, current version is {current}",
                    requirement.trim()
                );
            }
        }
    }
    Ok(root)
}

pub fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Control file must be a regular file: {}", path.display());
    }
    Ok(())
}

impl Engine {
    pub fn new(paths: &Paths, overrides: &[String], init: bool) -> Result<Self> {
        let root = source_root(paths)?;
        let mut env = Environment::new();
        env.set_keep_trailing_newline(true);
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.add_function("sh_quote", super::quoting::sh_quote);
        env.add_function("git_quote", super::quoting::git_quote);
        env.add_function("fish_quote", super::quoting::fish_quote);
        let directory = root.join(".stooltemplates");
        let template_root = root.clone();
        env.set_loader(move |name| {
            let relative = storage::relative_path(Path::new(name)).map_err(error)?;
            let path = directory.join(&relative);
            if path.exists() {
                let directory = fs::canonicalize(&directory).map_err(error)?;
                if !directory.starts_with(&template_root) {
                    return Err(error("Templates directory escapes source root"));
                }
                let resolved = fs::canonicalize(&path).map_err(error)?;
                if !resolved.starts_with(directory) {
                    return Err(error("Template include escapes the templates directory"));
                }
                return fs::read_to_string(resolved).map(Some).map_err(error);
            }
            Ok(None)
        });
        env.add_function("env", |name: String| {
            std::env::var(name).unwrap_or_default()
        });
        env.add_function("join_path", |parts: Rest<String>| {
            parts
                .iter()
                .collect::<PathBuf>()
                .to_string_lossy()
                .into_owned()
        });
        env.add_filter(
            "from_json",
            |text: String| -> std::result::Result<TemplateValue, Error> {
                Ok(TemplateValue::from_serialize(
                    serde_json::from_str::<Value>(&text).map_err(error)?,
                ))
            },
        );
        env.add_filter(
            "from_yaml",
            |text: String| -> std::result::Result<TemplateValue, Error> {
                Ok(TemplateValue::from_serialize(
                    serde_yaml::from_str::<Value>(&text).map_err(error)?,
                ))
            },
        );
        env.add_filter("to_yaml", |value: TemplateValue| {
            serde_yaml::to_string(&value).map_err(error)
        });
        env.add_filter("to_toml", |value: TemplateValue| {
            toml::to_string(&value).map_err(error)
        });
        let secrets = Secrets::default();
        let generic = secrets.clone();
        env.add_function(
            "secret",
            move |provider: String,
                  args: Rest<TemplateValue>|
                  -> std::result::Result<TemplateValue, Error> {
                let args: Vec<_> = args
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                    })
                    .collect();
                Ok(TemplateValue::from_serialize(
                    generic.get(&provider, &args).map_err(error)?,
                ))
            },
        );
        for provider in [
            "bitwarden",
            "pass",
            "gopass",
            "onepassword",
            "keyring",
            "vault",
            "keepassxc",
            "lastpass",
            "doppler",
            "aws",
            "azure",
        ] {
            let secrets = secrets.clone();
            env.add_function(
                provider,
                move |args: Rest<TemplateValue>| -> std::result::Result<TemplateValue, Error> {
                    let args: Vec<_> = args
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string())
                        })
                        .collect();
                    Ok(TemplateValue::from_serialize(
                        secrets.get(provider, &args).map_err(error)?,
                    ))
                },
            );
        }
        if init {
            env.add_function(
                "prompt_string",
                |message: String, default: Option<String>| prompt(&message, default).map_err(error),
            );
            env.add_function(
                "prompt_bool",
                |message: String, default: Option<bool>| -> std::result::Result<bool, Error> {
                    match prompt(&message, default.map(|v| v.to_string()))
                        .map_err(error)?
                        .to_lowercase()
                        .as_str()
                    {
                        "true" | "yes" | "y" | "1" => Ok(true),
                        "false" | "no" | "n" | "0" => Ok(false),
                        _ => Err(error("Expected yes/no")),
                    }
                },
            );
            env.add_function(
                "prompt_int",
                |message: String, default: Option<i64>| -> std::result::Result<i64, Error> {
                    prompt(&message, default.map(|v| v.to_string()))
                        .map_err(error)?
                        .parse()
                        .map_err(error)
                },
            );
            env.add_function(
                "prompt_choice",
                |message: String,
                 choices: Vec<String>,
                 default: Option<String>|
                 -> std::result::Result<String, Error> {
                    let value = prompt(&format!("{message} ({})", choices.join(", ")), default)
                        .map_err(error)?;
                    if !choices.contains(&value) {
                        return Err(error("Value is not one of the choices"));
                    }
                    Ok(value)
                },
            );
        }
        let hostname = std::process::Command::new("hostname")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        let builtin = json!({"os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "hostname": hostname, "username": std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default(),
            "home_dir": paths.home, "source_dir": root, "dest_dir": paths.destination,
            "cache_dir": paths.root.join("cache"), "config_file": paths.root.join("config.toml"),
            "version": env!("CARGO_PKG_VERSION"), "path_separator": std::path::MAIN_SEPARATOR.to_string()});
        let local = paths
            .config
            .as_ref()
            .map(|c| c.data.clone())
            .unwrap_or_else(storage::empty_object);
        let mut engine = Self {
            env,
            data: json!({"stool": builtin.clone()}),
        };
        let mut data_files = Vec::new();
        for extension in ["json", "yaml", "yml", "toml"] {
            for suffix in ["", ".tmpl"] {
                let path = root.join(format!(".stooldata.{extension}{suffix}"));
                if path.exists() {
                    data_files.push(path);
                }
            }
        }
        let directory = root.join(".stooldata");
        if directory.exists() {
            collect_data(&directory, &mut data_files)?;
        }
        for path in data_files {
            regular(&path)?;
            let bytes = fs::read(&path)?;
            let is_template = path.extension().is_some_and(|ext| ext == "tmpl");
            let data = if is_template {
                engine.render(&path.to_string_lossy(), &bytes)?
            } else {
                bytes
            };
            let name = path.file_name().unwrap().to_string_lossy();
            let name = name.strip_suffix(".tmpl").unwrap_or(&name);
            let value = parse_data(name, &data)
                .with_context(|| format!("Parse data file {}", path.display()))?;
            if !value.is_object() {
                bail!("Data file must contain an object: {}", path.display());
            }
            merge(&mut engine.data, value);
            merge(&mut engine.data, local.clone());
            engine.data["stool"] = builtin.clone();
        }
        merge(&mut engine.data, local);
        for entry in overrides {
            let (name, raw) = entry.split_once('=').context("--data requires key=value")?;
            if name.split('.').any(str::is_empty) || name == "stool" || name.starts_with("stool.") {
                bail!("--data cannot replace reserved stool data");
            }
            let value = serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()));
            let mut target = &mut engine.data;
            let components: Vec<_> = name.split('.').collect();
            for key in &components[..components.len() - 1] {
                if !target.is_object() {
                    bail!("--data path crosses a scalar");
                }
                if target.get(*key).is_none() {
                    target[*key] = json!({});
                }
                target = target
                    .get_mut(*key)
                    .context("--data path crosses a scalar")?;
            }
            if !target.is_object() {
                bail!("--data path crosses a scalar");
            }
            target[components[components.len() - 1]] = value;
        }
        engine.data["stool"] = builtin;
        Ok(engine)
    }

    pub fn render(&self, name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let text =
            std::str::from_utf8(bytes).with_context(|| format!("Template {name} must be UTF-8"))?;
        let mut source = String::new();
        let mut directives = std::collections::BTreeMap::new();
        for line in text.split_inclusive('\n') {
            if let Some(index) = line.find("stool:template:") {
                let tail = &line[index..];
                let tail = tail
                    .split_once("template:")
                    .unwrap()
                    .1
                    .trim()
                    .trim_end_matches("-->")
                    .trim();
                for part in shell_words::split(tail)? {
                    let (key, value) = part
                        .split_once('=')
                        .context("Template directives require key=value")?;
                    if ![
                        "left-delimiter",
                        "right-delimiter",
                        "line-ending",
                        "encoding",
                        "missing-key",
                    ]
                    .contains(&key)
                    {
                        bail!("Unknown template directive {key} in {name}");
                    }
                    directives.insert(key.to_owned(), value.to_owned());
                }
            } else {
                source.push_str(line);
            }
        }
        let mut env = self.env.clone();
        if directives.contains_key("left-delimiter") || directives.contains_key("right-delimiter") {
            let syntax = minijinja::syntax::SyntaxConfig::builder()
                .variable_delimiters(
                    directives
                        .get("left-delimiter")
                        .cloned()
                        .unwrap_or_else(|| "{{".into()),
                    directives
                        .get("right-delimiter")
                        .cloned()
                        .unwrap_or_else(|| "}}".into()),
                )
                .build()?;
            env.set_syntax(syntax);
        }
        match directives.get("missing-key").map(String::as_str) {
            None | Some("error") => {}
            Some("zero") | Some("default") => {
                env.set_undefined_behavior(UndefinedBehavior::Lenient)
            }
            Some(_) => bail!("missing-key must be error, zero, or default"),
        }
        let mut output = env
            .template_from_str(&source)
            .with_context(|| format!("Compile template {name}"))?
            .render(&self.data)
            .with_context(|| format!("Render template {name}"))?;
        match directives.get("line-ending").map(String::as_str) {
            None | Some("preserve") => {}
            Some("lf") | Some("unix") => output = output.replace("\r\n", "\n"),
            Some("crlf") | Some("windows") => {
                output = output.replace("\r\n", "\n").replace('\n', "\r\n")
            }
            Some(_) => bail!("Unsupported template line-ending"),
        }
        match directives
            .get("encoding")
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            None | Some("utf-8") | Some("utf8") => Ok(output.into_bytes()),
            Some("utf-16le") => Ok(output.encode_utf16().flat_map(u16::to_le_bytes).collect()),
            Some("utf-16be") => Ok(output.encode_utf16().flat_map(u16::to_be_bytes).collect()),
            Some(label) => {
                let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
                    .context("Unknown template encoding")?;
                let (bytes, _, errors) = encoding.encode(&output);
                if errors {
                    bail!("Template {name} contains characters unavailable in {label}");
                }
                Ok(bytes.into_owned())
            }
        }
    }
}

pub fn parse_data(name: &str, bytes: &[u8]) -> Result<Value> {
    match Path::new(name).extension().and_then(|s| s.to_str()) {
        Some("toml") => Ok(serde_json::to_value(toml::from_str::<toml::Value>(
            std::str::from_utf8(bytes)?,
        )?)?),
        Some("json") => Ok(serde_json::from_slice(bytes)?),
        Some("yaml" | "yml") => Ok(serde_yaml::from_slice(bytes)?),
        _ => bail!("Data files require .toml, .json, .yaml, or .yml"),
    }
}

fn collect_data(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        bail!("Data directories must not be symlinks");
    }
    let mut children = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(|e| e.file_name());
    for entry in children {
        if entry.file_type()?.is_dir() {
            collect_data(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn prompt(message: &str, default: Option<String>) -> Result<String> {
    if !io::stdin().is_terminal() {
        return default.context("A required init prompt has no default in non-interactive mode");
    }
    eprint!(
        "{message}{}: ",
        default
            .as_ref()
            .map(|v| format!(" [{v}]"))
            .unwrap_or_default()
    );
    io::stderr().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        default.context("A value is required")
    } else {
        Ok(value.to_owned())
    }
}

pub fn initialize_config(paths: &Paths, overrides: &[String]) -> Result<bool> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Patch {
        data: Option<Value>,
        encryption: Option<storage::EncryptionSettings>,
        git: Option<storage::GitSettings>,
        editor: Option<String>,
    }
    let root = source_root(paths)?;
    let name = ".stool.toml.tmpl";
    let file = root.join(name);
    if !file.exists() {
        return Ok(false);
    }
    regular(&file)?;
    let engine = Engine::new(paths, overrides, true)?;
    let rendered = engine.render(name, &fs::read(file)?)?;
    let patch: Patch = toml::from_str(std::str::from_utf8(&rendered)?)
        .context("Parse rendered init configuration")?;
    let mut config: Config = paths
        .config
        .clone()
        .context("Initialize Git configuration first")?;
    if let Some(data) = patch.data {
        if !data.is_object() {
            bail!("Init data must be an object");
        }
        merge(&mut config.data, data);
    }
    if let Some(encryption) = patch.encryption {
        config.encryption = encryption;
    }
    if let Some(git) = patch.git {
        config.git = git;
    }
    if patch.editor.is_some() {
        config.editor = patch.editor;
    }
    paths.save_config(&config)?;
    Ok(true)
}
