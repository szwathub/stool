use super::crypto;
use super::storage::{Fingerprint, Paths};
use super::template::{self, Engine};
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use globset::{GlobBuilder, GlobMatcher};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub enum Content {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
    Absent,
    Create(Vec<u8>),
    Modify { name: String, script: Vec<u8> },
    Remove { empty_only: bool },
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub relative: PathBuf,
    pub content: Content,
    pub mode: u32,
    pub origin: Option<PathBuf>,
}
impl Entry {
    pub fn new(relative: PathBuf, content: Content, mode: u32) -> Self {
        Self {
            relative,
            content,
            mode,
            origin: None,
        }
    }
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        match &self.content {
            Content::Directory => Some(Fingerprint::Directory { mode: self.mode }),
            Content::File(data) | Content::Create(data) => Some(Fingerprint::File {
                hash: filesystem::digest(data),
                mode: self.mode,
            }),
            Content::Symlink(target) => Some(Fingerprint::Symlink {
                target: target.clone(),
            }),
            Content::Absent | Content::Remove { .. } => None,
            Content::Modify { .. } => {
                unreachable!("modify transforms must be evaluated before fingerprinting")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Always,
    Once,
    OnChange,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Order {
    Before,
    During,
    After,
}
#[derive(Clone, Debug)]
pub struct Script {
    pub name: String,
    pub sort_key: PathBuf,
    pub bytes: Vec<u8>,
    pub condition: Condition,
    pub order: Order,
}
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    File,
    Symlink,
    Create,
    Modify,
    Remove,
    Script(Condition, Order),
}
#[derive(Debug)]
pub struct Attributes {
    pub name: String,
    pub kind: Kind,
    pub private: bool,
    pub readonly: bool,
    pub executable: bool,
    pub empty: bool,
    pub encrypted: bool,
    pub template: bool,
    pub encryption_extension: String,
    pub exact: bool,
    pub external: bool,
}

pub fn attributes(raw: &str, directory: bool) -> Result<Attributes> {
    let mut name = raw;
    let mut attr = Attributes {
        name: String::new(),
        kind: Kind::File,
        private: false,
        readonly: false,
        executable: false,
        empty: false,
        encrypted: false,
        template: false,
        encryption_extension: String::new(),
        exact: false,
        external: false,
    };
    if let Some(rest) = name.strip_prefix("remove_") {
        attr.kind = Kind::Remove;
        name = rest;
    } else if !directory {
        for (prefix, kind) in [
            ("create_", Kind::Create),
            ("modify_", Kind::Modify),
            ("symlink_", Kind::Symlink),
        ] {
            if let Some(rest) = name.strip_prefix(prefix) {
                attr.kind = kind;
                name = rest;
                break;
            }
        }
        if let Some(rest) = name.strip_prefix("run_") {
            name = rest;
            let condition = if let Some(rest) = name.strip_prefix("once_") {
                name = rest;
                Condition::Once
            } else if let Some(rest) = name.strip_prefix("onchange_") {
                name = rest;
                Condition::OnChange
            } else {
                Condition::Always
            };
            let order = if let Some(rest) = name.strip_prefix("before_") {
                name = rest;
                Order::Before
            } else if let Some(rest) = name.strip_prefix("after_") {
                name = rest;
                Order::After
            } else {
                Order::During
            };
            attr.kind = Kind::Script(condition, order);
        }
    }
    let mut dot = false;
    let mut previous = 0;
    loop {
        if let Some(rest) = name.strip_prefix("literal_") {
            name = rest;
            break;
        }
        let prefixes: &[(&str, usize)] = if directory {
            &[
                ("external_", 1),
                ("exact_", 2),
                ("private_", 3),
                ("readonly_", 4),
                ("dot_", 7),
            ]
        } else {
            &[
                ("encrypted_", 1),
                ("private_", 3),
                ("readonly_", 4),
                ("empty_", 5),
                ("executable_", 6),
                ("dot_", 7),
            ]
        };
        if let Some((prefix, order)) = prefixes.iter().find(|(prefix, _)| name.starts_with(prefix))
        {
            if *order <= previous
                || matches!(attr.kind, Kind::Script(..))
                || (matches!(attr.kind, Kind::Symlink | Kind::Remove)
                    && !directory
                    && *prefix != "dot_")
                || (matches!(attr.kind, Kind::Modify) && *prefix == "empty_")
            {
                bail!("Invalid attribute order or type: {raw}");
            }
            previous = *order;
            match *prefix {
                "external_" => attr.external = true,
                "exact_" => attr.exact = true,
                "encrypted_" => attr.encrypted = true,
                "private_" => attr.private = true,
                "readonly_" => attr.readonly = true,
                "empty_" => attr.empty = true,
                "executable_" => attr.executable = true,
                "dot_" => dot = true,
                _ => unreachable!(),
            }
            name = &name[prefix.len()..];
            continue;
        }
        const RESERVED: &[&str] = &[
            "encrypted_",
            "readonly_",
            "private_",
            "empty_",
            "executable_",
            "dot_",
            "create_",
            "modify_",
            "remove_",
            "run_",
            "once_",
            "onchange_",
            "before_",
            "after_",
            "exact_",
            "external_",
            "symlink_",
        ];
        if RESERVED.iter().any(|prefix| name.starts_with(prefix)) {
            bail!("Invalid attribute order or type: {raw}; use literal_ for a literal filename");
        }
        break;
    }
    if !directory {
        // Encryption wrapping is outermost; literal suffixes stop further suffix parsing.
        if attr.encrypted {
            for extension in ["age", "asc"] {
                if let Some(rest) = name.strip_suffix(&format!(".{extension}")) {
                    name = rest;
                    attr.encryption_extension = extension.into();
                    break;
                }
            }
        }
        if let Some(rest) = name.strip_suffix(".literal") {
            name = rest;
        } else if let Some(rest) = name.strip_suffix(".tmpl") {
            if matches!(attr.kind, Kind::Remove) {
                bail!("remove_ entries cannot be templates");
            }
            name = rest;
            attr.template = true;
            if let Some(rest) = name.strip_suffix(".literal") {
                name = rest;
            }
            if attr.encrypted && attr.encryption_extension.is_empty() {
                for extension in ["age", "asc"] {
                    if let Some(rest) = name.strip_suffix(&format!(".{extension}")) {
                        name = rest;
                        attr.encryption_extension = extension.into();
                        break;
                    }
                }
            }
        }
    }
    if name.is_empty() {
        bail!("Empty target name in {raw}");
    }
    attr.name = if dot {
        format!(".{name}")
    } else {
        name.to_owned()
    };
    let mut components = Path::new(&attr.name).components();
    if attr.name.contains(['/', '\\'])
        || attr.name.chars().any(char::is_control)
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        bail!("Invalid target name in {raw}");
    }
    Ok(attr)
}

#[derive(Clone)]
struct Rule {
    base: PathBuf,
    matcher: GlobMatcher,
    exception: bool,
    directory_only: bool,
}
fn parse_rules(text: &str, base: &Path, label: &str) -> Result<Vec<Rule>> {
    let mut result = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let end = raw
            .char_indices()
            .find(|(i, c)| *c == '#' && (*i == 0 || raw[..*i].ends_with(char::is_whitespace)))
            .map(|(i, _)| i)
            .unwrap_or(raw.len());
        let line = raw[..end].trim();
        if line.is_empty() {
            continue;
        }
        let (exception, pattern) = match line.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, line),
        };
        let directory_only = pattern.ends_with('/');
        let pattern = pattern.trim_start_matches('/').trim_end_matches('/');
        if pattern.is_empty() {
            bail!("Empty ignore pattern at {label}:{}", index + 1);
        }
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
            .with_context(|| format!("Invalid pattern at {label}:{}", index + 1))?
            .compile_matcher();
        result.push(Rule {
            base: base.to_owned(),
            matcher,
            exception,
            directory_only,
        });
    }
    Ok(result)
}
fn rules(directory: &Path, base: &Path, engine: &Engine) -> Result<Vec<Rule>> {
    let mut result = Vec::new();
    for name in [".stoolignore", ".stoolignore.tmpl"] {
        let path = directory.join(name);
        if path.exists() {
            template::regular(&path)?;
            let text = engine.render(&path.to_string_lossy(), &fs::read(&path)?)?;
            result.extend(parse_rules(std::str::from_utf8(&text)?, base, name)?);
        }
    }
    Ok(result)
}
fn ignored(relative: &Path, directory: bool, rules: &[Rule]) -> bool {
    let mut included = false;
    for rule in rules {
        if rule.directory_only && !directory {
            continue;
        }
        if let Ok(local) = relative.strip_prefix(&rule.base) {
            if rule.matcher.is_match(local) {
                if rule.exception {
                    return false;
                }
                included = true;
            }
        }
    }
    included
}

pub fn ignored_external(
    tree: &Tree,
    engine: &Engine,
    relative: &Path,
    directory: bool,
) -> Result<bool> {
    let mut active = rules(&tree.root, Path::new(""), engine)?;
    let mut parent = relative.parent();
    while let Some(path) = parent {
        if let Some(origin) = tree
            .entries
            .get(path)
            .and_then(|entry| entry.origin.as_ref())
        {
            if origin.is_dir() {
                active.extend(rules(origin, path, engine)?);
            }
        }
        parent = path.parent();
    }
    Ok(ignored(relative, directory, &active))
}

pub struct Tree {
    pub entries: BTreeMap<PathBuf, Entry>,
    pub scripts: Vec<Script>,
    pub exact: BTreeSet<PathBuf>,
    pub removals: Vec<String>,
    pub root: PathBuf,
}
impl Tree {
    pub fn insert(&mut self, entry: Entry) -> Result<()> {
        if self.entries.insert(entry.relative.clone(), entry).is_some() {
            bail!("Multiple source entries map to the same target");
        }
        Ok(())
    }
}

pub fn scan(paths: &Paths, engine: &Engine, materialize: bool) -> Result<Tree> {
    let root = template::source_root(paths)?;
    let mut tree = Tree {
        entries: BTreeMap::new(),
        scripts: Vec::new(),
        exact: BTreeSet::new(),
        removals: Vec::new(),
        root: root.clone(),
    };
    walk(
        paths,
        engine,
        &root,
        Path::new(""),
        &[],
        false,
        materialize,
        &mut tree,
    )?;
    for name in [".stoolremove", ".stoolremove.tmpl"] {
        let path = root.join(name);
        if path.exists() {
            template::regular(&path)?;
            let output = engine.render(name, &fs::read(&path)?)?;
            for line in std::str::from_utf8(&output)?.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    tree.removals.push(line.into());
                }
            }
        }
    }
    tree.scripts
        .sort_by(|a, b| (a.order, &a.sort_key, &a.name).cmp(&(b.order, &b.sort_key, &b.name)));
    Ok(tree)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    paths: &Paths,
    engine: &Engine,
    directory: &Path,
    target_base: &Path,
    inherited: &[Rule],
    literal_children: bool,
    materialize: bool,
    tree: &mut Tree,
) -> Result<()> {
    let mut active = inherited.to_vec();
    active.extend(rules(directory, target_base, engine)?);
    let mut children = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let raw = child
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-UTF-8 source filename"))?;
        let source = child.path();
        let meta = fs::symlink_metadata(&source)?;
        if raw.starts_with(".stool") {
            if raw == ".stoolscripts" {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    bail!("Scripts directory must be a regular directory");
                }
                scripts_directory(paths, engine, &source, materialize, tree)?;
            } else if !known_control(&raw) {
                bail!("Unsupported control file: {}", source.display());
            }
            continue;
        }
        if raw == ".git" || (!literal_children && raw.starts_with('.')) {
            continue;
        }
        if meta.file_type().is_symlink() && !literal_children {
            bail!(
                "Native source symlinks are unsupported; use a regular symlink_ file: {}",
                source.display()
            );
        }
        let attr = if literal_children {
            Attributes {
                name: raw.clone(),
                kind: Kind::File,
                private: false,
                readonly: false,
                executable: false,
                empty: true,
                encrypted: false,
                template: false,
                encryption_extension: String::new(),
                exact: false,
                external: true,
            }
        } else {
            attributes(&raw, meta.is_dir())
                .with_context(|| format!("Parse {}", source.display()))?
        };
        let relative = target_base.join(&attr.name);
        let is_ignored = ignored(&relative, meta.is_dir(), &active);
        if !is_ignored {
            let mut bytes = if meta.is_file() && materialize {
                fs::read(&source)?
            } else if meta.is_file() && meta.len() == 0 {
                Vec::new()
            } else {
                vec![b'x']
            };
            if materialize && bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
                bail!(
                    "Git LFS object is not checked out: {}; enable git.lfs and run update",
                    source.display()
                );
            }
            if materialize && attr.encrypted {
                bytes = crypto::decrypt(
                    &paths.encryption(),
                    &paths.home,
                    &bytes,
                    &attr.encryption_extension,
                )
                .with_context(|| format!("Decrypt {}", source.display()))?;
            }
            if materialize && attr.template {
                bytes = engine.render(&source.to_string_lossy(), &bytes)?;
            }
            if let Kind::Script(condition, order) = attr.kind {
                tree.scripts.push(Script {
                    sort_key: relative.clone(),
                    name: source
                        .strip_prefix(&tree.root)?
                        .to_string_lossy()
                        .into_owned(),
                    bytes,
                    condition,
                    order,
                });
                continue;
            }
            let content = if matches!(attr.kind, Kind::Remove) {
                Content::Remove { empty_only: true }
            } else if literal_children && meta.file_type().is_symlink() {
                Content::Symlink(fs::read_link(&source)?)
            } else if meta.is_dir() {
                Content::Directory
            } else if !meta.is_file() {
                bail!("Unsupported source type: {}", source.display());
            } else {
                match attr.kind {
                    Kind::Symlink => {
                        let text =
                            String::from_utf8(bytes).context("Symlink target must be UTF-8")?;
                        let text = text.trim_end_matches(['\r', '\n']);
                        if text.is_empty() || text.contains(['\0', '\r', '\n']) {
                            bail!("Invalid symlink target: {}", source.display());
                        }
                        Content::Symlink(PathBuf::from(text))
                    }
                    Kind::Create => Content::Create(bytes),
                    Kind::Modify => Content::Modify {
                        name: raw.clone(),
                        script: bytes,
                    },
                    _ if bytes.is_empty() && !attr.empty => Content::Absent,
                    _ => Content::File(bytes),
                }
            };
            let mut mode = if meta.is_dir() || attr.executable {
                0o755
            } else {
                0o644
            };
            if literal_children {
                mode = filesystem::mode(&meta);
            }
            if attr.private {
                mode &= 0o700;
            }
            if attr.readonly {
                mode &= !0o222;
            }
            let mut entry = Entry::new(relative.clone(), content, mode);
            entry.origin = Some(source.clone());
            tree.insert(entry)?;
            if attr.exact {
                tree.exact.insert(relative.clone());
            }
        }
        if meta.is_dir() && !matches!(attr.kind, Kind::Remove) {
            walk(
                paths,
                engine,
                &source,
                &relative,
                &active,
                literal_children || attr.external,
                materialize,
                tree,
            )?;
        }
    }
    Ok(())
}

fn scripts_directory(
    paths: &Paths,
    engine: &Engine,
    directory: &Path,
    materialize: bool,
    tree: &mut Tree,
) -> Result<()> {
    let mut children = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let name = child
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-UTF-8 script name"))?;
        if name.starts_with('.') {
            continue;
        }
        if child.file_type()?.is_dir() {
            scripts_directory(paths, engine, &child.path(), materialize, tree)?;
            continue;
        }
        template::regular(&child.path())?;
        let canonical = if name.starts_with("run_") {
            name
        } else {
            format!("run_{name}")
        };
        let attr = attributes(&canonical, false)?;
        let Kind::Script(condition, order) = attr.kind else {
            unreachable!()
        };
        let bytes = if materialize {
            fs::read(child.path())?
        } else {
            Vec::new()
        };
        let bytes = if materialize && attr.template {
            engine.render(&canonical, &bytes)?
        } else {
            bytes
        };
        tree.scripts.push(Script {
            sort_key: PathBuf::from(&attr.name),
            name: child
                .path()
                .strip_prefix(&tree.root)?
                .to_string_lossy()
                .into_owned(),
            bytes,
            condition,
            order,
        });
    }
    let _ = paths;
    Ok(())
}

pub fn known_control(name: &str) -> bool {
    let Some(stripped) = name.strip_prefix(".stool") else {
        return false;
    };
    matches!(
        stripped,
        "root" | "version" | "templates" | "data" | "scripts" | "externals"
    ) || ["ignore", "remove"]
        .iter()
        .any(|prefix| stripped == *prefix || stripped == format!("{prefix}.tmpl"))
        || ["data", "external"].iter().any(|prefix| {
            ["toml", "json", "yaml", "yml"].iter().any(|ext| {
                stripped == format!("{prefix}.{ext}") || stripped == format!("{prefix}.{ext}.tmpl")
            })
        })
        || stripped == ".toml.tmpl"
}

#[cfg(test)]
#[path = "../../tests/unit/dotfile/source.rs"]
mod tests;
