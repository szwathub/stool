use super::source::{Content, Entry, Tree};
use super::storage::Paths;
use super::template::Engine;
use super::{archive, git, source, storage, template};
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct Resource {
    #[serde(rename = "type")]
    kind: String,
    url: String,
    refresh_period: Option<String>,
    strip_components: usize,
    exact: bool,
    executable: bool,
    private: bool,
    include: Vec<String>,
    exclude: Vec<String>,
    checksum: Checksum,
    branch: Option<String>,
    recurse_submodules: bool,
    path: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Checksum {
    sha256: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Cache {
    fetched_at: u64,
}

pub fn load(paths: &Paths, engine: &Engine, tree: &mut Tree, write_cache: bool) -> Result<()> {
    let mut resources: BTreeMap<String, Resource> = BTreeMap::new();
    for extension in ["json", "yaml", "yml", "toml"] {
        for suffix in ["", ".tmpl"] {
            let name = format!(".stoolexternal.{extension}{suffix}");
            let path = tree.root.join(&name);
            if path.exists() {
                template::regular(&path)?;
                let rendered = engine.render(&name, &fs::read(path)?)?;
                let data = template::parse_data(&format!("external.{extension}"), &rendered)?;
                resources.extend(serde_json::from_value::<BTreeMap<String, Resource>>(data)?);
            }
        }
    }
    let directory = tree.root.join(".stoolexternals");
    if directory.exists() {
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            bail!("External definitions directory cannot be a symlink");
        }
        let mut files = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
        files.sort_by_key(|entry| entry.file_name());
        for file in files {
            template::regular(&file.path())?;
            let name = file.file_name().to_string_lossy().into_owned();
            let rendered = engine.render(&name, &fs::read(file.path())?)?;
            resources.extend(serde_json::from_value::<BTreeMap<String, Resource>>(
                template::parse_data(name.strip_suffix(".tmpl").unwrap_or(&name), &rendered)?,
            )?);
        }
    }
    for (name, resource) in resources {
        let relative = storage::relative_path(Path::new(&name))?;
        paths.check_target(&paths.destination.join(&relative))?;
        if source::ignored_external(
            tree,
            engine,
            &relative,
            resource.kind != "file" && resource.kind != "archive-file",
        )? {
            continue;
        }
        if resource.url.is_empty() {
            bail!("External resource {name} has no URL");
        }
        let kind = if resource.kind.is_empty() {
            "file"
        } else {
            &resource.kind
        };
        let ttl = duration(resource.refresh_period.as_deref().unwrap_or("24h"))?;
        let key = filesystem::digest(&serde_json::to_vec(&(
            &resource.url,
            kind,
            &resource.branch,
            resource.recurse_submodules,
        ))?);
        let cache = paths.root.join("cache/external").join(key);
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let fresh = fs::read(cache.join("metadata.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
            .is_some_and(|metadata| now.saturating_sub(metadata.fetched_at) < ttl);
        let temp = tempfile::Builder::new()
            .prefix("stool-external-")
            .tempdir()?;
        if kind == "git-repo" {
            let repository = if fresh && cache.join("repository").is_dir() {
                cache.join("repository")
            } else {
                let checkout = temp.path().join("repository");
                git::clone_repository(
                    &resource.url,
                    &checkout,
                    resource.branch.as_deref(),
                    resource.recurse_submodules,
                )?;
                if write_cache {
                    cache_directory(paths, &cache)?;
                    if cache.join("repository").exists() {
                        fs::remove_dir_all(cache.join("repository"))?;
                    }
                    fs::rename(&checkout, cache.join("repository"))?;
                    filesystem::atomic_write(
                        &cache.join("metadata.json"),
                        &serde_json::to_vec(&Cache { fetched_at: now })?,
                        0o600,
                    )?;
                    cache.join("repository")
                } else {
                    checkout
                }
            };
            let mut entries = Vec::new();
            read_repository(&repository, Path::new(""), &mut entries)?;
            insert_entries(paths, engine, tree, &relative, entries, &resource)?;
        } else {
            let bytes = if fresh && cache.join("payload").is_file() {
                template::regular(&cache.join("payload"))?;
                fs::read(cache.join("payload"))?
            } else {
                let bytes = download(&resource.url)?;
                if let Some(expected) = &resource.checksum.sha256 {
                    if filesystem::digest(&bytes) != expected.to_lowercase() {
                        bail!("External checksum mismatch: {name}");
                    }
                }
                if write_cache {
                    cache_directory(paths, &cache)?;
                    filesystem::atomic_write(&cache.join("payload"), &bytes, 0o600)?;
                    filesystem::atomic_write(
                        &cache.join("metadata.json"),
                        &serde_json::to_vec(&Cache { fetched_at: now })?,
                        0o600,
                    )?;
                }
                bytes
            };
            if let Some(expected) = &resource.checksum.sha256 {
                if filesystem::digest(&bytes) != expected.to_lowercase() {
                    bail!("External checksum mismatch: {name}");
                }
            }
            match kind {
                "file" => {
                    let mode = if resource.private {
                        if resource.executable {
                            0o700
                        } else {
                            0o600
                        }
                    } else if resource.executable {
                        0o755
                    } else {
                        0o644
                    };
                    tree.insert(Entry::new(relative, Content::File(bytes), mode))?;
                }
                "archive" => insert_entries(
                    paths,
                    engine,
                    tree,
                    &relative,
                    archive::read(&bytes, resource.strip_components)?,
                    &resource,
                )?,
                "archive-file" => {
                    let selected = resource
                        .path
                        .as_ref()
                        .context("archive-file requires path")?;
                    let entries = archive::read(&bytes, resource.strip_components)?;
                    let mut entry = entries
                        .into_iter()
                        .find(|entry| entry.relative == Path::new(selected))
                        .context("archive-file member was not found")?;
                    if !matches!(entry.content, Content::File(_)) {
                        bail!("archive-file must select a regular file");
                    }
                    entry.relative = relative;
                    if resource.private {
                        entry.mode &= 0o700;
                    }
                    if resource.executable {
                        entry.mode |= 0o111;
                    }
                    tree.insert(entry)?;
                }
                _ => bail!("Unknown external type: {kind}"),
            }
        }
    }
    Ok(())
}
fn cache_directory(paths: &Paths, directory: &Path) -> Result<()> {
    filesystem::private_dir(&paths.root.join("cache"))?;
    filesystem::private_dir(&paths.root.join("cache/external"))?;
    filesystem::private_dir(directory)
}
fn download(url: &str) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        #[cfg(windows)]
        let path = if path.starts_with('/') && path.as_bytes().get(2) == Some(&b':') {
            &path[1..]
        } else {
            path
        };
        return Ok(fs::read(path)?);
    }
    if !url.starts_with("https://") && !url.starts_with("http://") {
        bail!("External URL must use http(s) or file");
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|_| anyhow::anyhow!("External HTTP download failed; check URL/access"))?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn insert_entries(
    paths: &Paths,
    engine: &Engine,
    tree: &mut Tree,
    prefix: &Path,
    entries: Vec<Entry>,
    resource: &Resource,
) -> Result<()> {
    let include: Vec<_> = resource
        .include
        .iter()
        .map(|pattern| globset::Glob::new(pattern).map(|glob| glob.compile_matcher()))
        .collect::<std::result::Result<_, _>>()?;
    let exclude: Vec<_> = resource
        .exclude
        .iter()
        .map(|pattern| globset::Glob::new(pattern).map(|glob| glob.compile_matcher()))
        .collect::<std::result::Result<_, _>>()?;
    if let Some(entry) = tree.entries.get_mut(prefix) {
        if !matches!(entry.content, Content::Directory) {
            bail!("External container collides with a source file");
        }
        if resource.private {
            entry.mode &= 0o700;
        }
    } else {
        tree.insert(Entry::new(
            prefix.to_owned(),
            Content::Directory,
            if resource.private { 0o700 } else { 0o755 },
        ))?;
    }
    if resource.exact {
        tree.exact.insert(prefix.to_owned());
    }
    for mut entry in entries {
        if (!include.is_empty()
            && !include
                .iter()
                .any(|matcher| matcher.is_match(&entry.relative))
            && !matches!(entry.content, Content::Directory))
            || exclude
                .iter()
                .any(|matcher| matcher.is_match(&entry.relative))
        {
            continue;
        }
        entry.relative = prefix.join(entry.relative);
        paths.check_target(&paths.destination.join(&entry.relative))?;
        if source::ignored_external(
            tree,
            engine,
            &entry.relative,
            matches!(entry.content, Content::Directory),
        )? {
            continue;
        }
        if resource.private {
            entry.mode &= 0o700;
        }
        tree.insert(entry)?;
    }
    Ok(())
}
fn read_repository(root: &Path, relative: &Path, entries: &mut Vec<Entry>) -> Result<()> {
    for child in fs::read_dir(root.join(relative))? {
        let child = child?;
        if child.file_name() == ".git" {
            continue;
        }
        let path = relative.join(child.file_name());
        let metadata = fs::symlink_metadata(child.path())?;
        let content = if metadata.file_type().is_symlink() {
            Content::Symlink(fs::read_link(child.path())?)
        } else if metadata.is_dir() {
            Content::Directory
        } else if metadata.is_file() {
            Content::File(fs::read(child.path())?)
        } else {
            bail!("External repository contains a special file");
        };
        entries.push(Entry::new(
            path.clone(),
            content,
            filesystem::mode(&metadata),
        ));
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            read_repository(root, &path, entries)?;
        }
    }
    Ok(())
}
fn duration(raw: &str) -> Result<u64> {
    if raw == "0" {
        return Ok(0);
    }
    let mut digits = String::new();
    let mut seconds = 0u64;
    for c in raw.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            let amount: u64 = digits.parse().context("Invalid external refreshPeriod")?;
            digits.clear();
            let unit = match c {
                's' => 1,
                'm' => 60,
                'h' => 3600,
                'd' => 86400,
                'w' => 604800,
                _ => bail!("Unsupported refreshPeriod unit"),
            };
            seconds = seconds
                .checked_add(amount.checked_mul(unit).context("refreshPeriod overflow")?)
                .context("refreshPeriod overflow")?;
        }
    }
    if !digits.is_empty() {
        seconds = seconds
            .checked_add(digits.parse()?)
            .context("refreshPeriod overflow")?;
    }
    Ok(seconds)
}
