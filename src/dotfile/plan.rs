use super::source::{Content, Entry, Order, Script, Tree};
use super::storage::{self, Fingerprint, Paths, State};
use super::{process, scripts};
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use similar::TextDiff;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Remove,
    Conflict,
    Unchanged,
}

#[derive(Clone)]
struct Snapshot {
    fingerprint: Option<Fingerprint>,
    bytes: Option<Vec<u8>>,
}

fn absent(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || (cfg!(unix) && error.raw_os_error() == Some(20))
    // ENOTDIR: a planned parent may replace a file.
}

pub fn actual_fingerprint(path: &Path) -> Result<Option<Fingerprint>> {
    Ok(snapshot(path)?.fingerprint)
}

fn snapshot(path: &Path) -> Result<Snapshot> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if absent(&error) => {
            return Ok(Snapshot {
                fingerprint: None,
                bytes: None,
            })
        }
        Err(error) => return Err(error).with_context(|| format!("Inspect {}", path.display())),
    };
    let mut bytes = None;
    let fingerprint = if meta.file_type().is_symlink() {
        Fingerprint::Symlink {
            target: fs::read_link(path)?,
        }
    } else if meta.is_dir() {
        Fingerprint::Directory {
            mode: filesystem::path_mode(path, &meta)?,
        }
    } else if meta.is_file() {
        let data = fs::read(path).with_context(|| format!("Read {}", path.display()))?;
        let value = Fingerprint::File {
            hash: filesystem::digest(&data),
            mode: filesystem::path_mode(path, &meta)?,
        };
        bytes = Some(data);
        value
    } else {
        bail!("Unsupported destination file type: {}", path.display());
    };
    Ok(Snapshot {
        fingerprint: Some(fingerprint),
        bytes,
    })
}

#[derive(Clone)]
pub struct Change {
    pub entry: Entry,
    actual: Snapshot,
    pub action: Action,
}

pub struct Plan {
    pub changes: Vec<Change>,
    pub scripts: Vec<Script>,
}

fn check_parents(
    paths: &Paths,
    relative: &Path,
    entries: Option<&BTreeMap<PathBuf, Entry>>,
) -> Result<()> {
    let mut parent = relative.parent();
    while let Some(relative_parent) = parent {
        let full = paths.destination.join(relative_parent);
        match fs::symlink_metadata(&full) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    bail!("Destination parent is a symlink: {}", full.display());
                }
                let planned_directory = entries
                    .and_then(|map| map.get(relative_parent))
                    .is_some_and(|e| matches!(e.content, Content::Directory));
                if !meta.is_dir() && !planned_directory {
                    bail!("Destination parent is not a directory: {}", full.display());
                }
            }
            Err(error) if absent(&error) => {}
            Err(error) => {
                return Err(error).with_context(|| format!("Inspect parent {}", full.display()))
            }
        }
        if relative_parent.as_os_str().is_empty() {
            break;
        }
        parent = relative_parent.parent();
    }
    Ok(())
}

impl Plan {
    pub fn build(paths: &Paths, tree: Tree, state: &State) -> Result<Self> {
        let Tree {
            mut entries,
            scripts,
            exact,
            removals,
            ..
        } = tree;
        let mut intentional = BTreeSet::new();
        for relative in entries.keys().cloned().collect::<Vec<_>>() {
            let entry = &entries[&relative];
            paths.check_target(&paths.destination.join(&relative))?;
            check_parents(paths, &relative, Some(&entries))?;
            let actual = snapshot(&paths.destination.join(&relative))?;
            let content = match &entry.content {
                Content::Create(bytes) => match &actual.fingerprint {
                    Some(Fingerprint::File { mode, .. }) => {
                        Some((Content::File(actual.bytes.unwrap()), *mode))
                    }
                    Some(Fingerprint::Directory { mode }) => Some((Content::Directory, *mode)),
                    Some(Fingerprint::Symlink { target }) => {
                        Some((Content::Symlink(target.clone()), entry.mode))
                    }
                    None => Some((Content::File(bytes.clone()), entry.mode)),
                },
                Content::Modify { name, script } => {
                    if actual
                        .fingerprint
                        .as_ref()
                        .is_some_and(|entry| !matches!(entry, Fingerprint::File { .. }))
                    {
                        bail!(
                            "modify_ requires a regular file or absent target: {}",
                            relative.display()
                        );
                    }
                    intentional.insert(relative.clone());
                    Some((
                        Content::File(process::script(
                            paths,
                            name,
                            script,
                            Some(actual.bytes.as_deref().unwrap_or_default()),
                            true,
                        )?),
                        entry.mode,
                    ))
                }
                Content::Remove { empty_only } => {
                    if *empty_only
                        && matches!(actual.fingerprint, Some(Fingerprint::Directory { .. }))
                        && fs::read_dir(paths.destination.join(&relative))?
                            .next()
                            .is_some()
                    {
                        bail!("remove_ directory is not empty: {}", relative.display());
                    }
                    intentional.insert(relative.clone());
                    Some((Content::Absent, entry.mode))
                }
                _ => None,
            };
            if let Some((content, mode)) = content {
                let entry = entries.get_mut(&relative).unwrap();
                entry.content = content;
                entry.mode = mode;
            }
        }
        let mut deletions = BTreeSet::new();
        for exact_root in exact {
            paths.check_target(&paths.destination.join(&exact_root))?;
            exact_extras(paths, &exact_root, &entries, &mut deletions)?;
        }
        if !removals.is_empty() {
            let mut matchers = Vec::new();
            for pattern in removals {
                if pattern.starts_with('/')
                    || pattern.starts_with('~')
                    || pattern.split('/').any(|part| part == "..")
                {
                    bail!("Removal patterns must stay inside destination");
                }
                matchers.push(
                    globset::GlobBuilder::new(pattern.trim_end_matches('/'))
                        .literal_separator(true)
                        .build()?
                        .compile_matcher(),
                );
            }
            collect_removals(paths, Path::new(""), &matchers, &mut deletions)?;
        }
        let deletions: Vec<_> = deletions.into_iter().collect();
        for relative in &deletions {
            if deletions
                .iter()
                .any(|parent| parent != relative && relative.starts_with(parent))
            {
                continue;
            }
            if entries.iter().any(|(name, entry)| {
                (name == relative || name.starts_with(relative))
                    && !matches!(entry.content, Content::Absent)
            }) {
                bail!(
                    "A target is both managed and marked for removal: {}",
                    relative.display()
                );
            }
            entries.insert(
                relative.clone(),
                Entry::new(relative.clone(), Content::Absent, 0o644),
            );
            intentional.insert(relative.clone());
        }
        // Ignoring a directory entry does not ignore its children. Ensure their
        // parents exist while preserving any existing directory permissions.
        for relative in entries.keys().cloned().collect::<Vec<_>>() {
            let mut parent = relative.parent();
            while let Some(path) = parent {
                if path.as_os_str().is_empty() {
                    break;
                }
                if let Some(entry) = entries.get(path) {
                    if !matches!(entry.content, Content::Directory) {
                        bail!(
                            "Target is also used as a parent directory: {}",
                            path.display()
                        );
                    }
                } else {
                    let mode = match snapshot(&paths.destination.join(path))?.fingerprint {
                        Some(Fingerprint::Directory { mode }) => mode,
                        _ => 0o755,
                    };
                    entries.insert(
                        path.to_owned(),
                        Entry {
                            relative: path.to_owned(),
                            content: Content::Directory,
                            mode,
                            origin: None,
                        },
                    );
                }
                parent = path.parent();
            }
        }
        let mut names = BTreeSet::new();
        let mut changes = Vec::new();
        for entry in entries.values_mut() {
            entry.mode = filesystem::normalized_mode(
                entry.mode,
                matches!(entry.content, Content::Directory),
            );
        }
        for (relative, entry) in &entries {
            storage::relative_path(relative)?;
            let target = paths.destination.join(relative);
            paths.check_target(&target)?;
            check_parents(paths, relative, Some(&entries))?;
            let name = relative.to_str().context("Target path must be UTF-8")?;
            let key = if cfg!(target_os = "macos") || cfg!(windows) {
                name.to_lowercase()
            } else {
                name.to_owned()
            };
            if !names.insert(key) {
                bail!("Case-colliding destination path: {name}");
            }
            let actual = snapshot(&target)?;
            let desired = entry.fingerprint();
            let previous = state.entries.get(name);
            let action = if actual.fingerprint == desired {
                Action::Unchanged
            } else if actual.fingerprint.as_ref() != previous
                && !(intentional.contains(relative) && previous.is_none())
            {
                Action::Conflict
            } else if desired.is_none() {
                Action::Remove
            } else if actual.fingerprint.is_none() {
                Action::Create
            } else {
                Action::Update
            };
            changes.push((relative.clone(), actual, action));
        }
        Ok(Self {
            scripts,
            changes: changes
                .into_iter()
                .map(|(path, actual, action)| Change {
                    entry: entries.remove(&path).unwrap(),
                    actual,
                    action,
                })
                .collect(),
        })
    }

    pub fn changed(&self) -> bool {
        self.changes
            .iter()
            .any(|change| change.action != Action::Unchanged)
    }

    pub fn conflicts(&self) -> bool {
        self.changes
            .iter()
            .any(|change| change.action == Action::Conflict)
    }

    pub fn show_scripts(&self, paths: &Paths) -> Result<()> {
        let store = scripts::Store::load(paths)?;
        for script in &self.scripts {
            if store.pending(script) {
                println!("S {:?} {}", script.order, script.name);
            }
        }
        Ok(())
    }

    pub fn show(&self, diff: bool) {
        for change in &self.changes {
            if change.action == Action::Unchanged {
                continue;
            }
            let marker = match change.action {
                Action::Create => 'A',
                Action::Update => 'M',
                Action::Remove => 'D',
                Action::Conflict => '!',
                Action::Unchanged => ' ',
            };
            println!("{marker} {}", change.entry.relative.display());
            if diff {
                let old = change.actual.bytes.as_deref().unwrap_or_default();
                let new = match &change.entry.content {
                    Content::File(data) => data.as_slice(),
                    _ => &[],
                };
                if let (Ok(before), Ok(after)) =
                    (std::str::from_utf8(old), std::str::from_utf8(new))
                {
                    if before != after {
                        let path = change.entry.relative.to_string_lossy();
                        print!(
                            "{}",
                            TextDiff::from_lines(before, after)
                                .unified_diff()
                                .header(&format!("a/{path}"), &format!("b/{path}"))
                        );
                    }
                } else {
                    println!("  binary contents changed");
                }
                let before = describe(change.actual.fingerprint.as_ref());
                let after = describe(change.entry.fingerprint().as_ref());
                if before != after {
                    println!("  {before} -> {after}");
                }
            }
        }
        let changed = self
            .changes
            .iter()
            .filter(|c| c.action != Action::Unchanged)
            .count();
        let conflicts = self
            .changes
            .iter()
            .filter(|c| c.action == Action::Conflict)
            .count();
        println!("{changed} change(s), {conflicts} conflict(s)");
    }

    pub fn apply(&self, paths: &Paths, state: &mut State, force: bool) -> Result<()> {
        let conflicts: Vec<_> = self
            .changes
            .iter()
            .filter(|c| c.action == Action::Conflict)
            .map(|c| c.entry.relative.display().to_string())
            .collect();
        if !force && !conflicts.is_empty() {
            bail!("Conflicting destination entries: {}. Inspect diff, then use apply --force to back up and overwrite", conflicts.join(", "));
        }
        if !self.changes.is_empty() || !self.scripts.is_empty() {
            fs::create_dir_all(&paths.destination)?;
        }
        let mut script_store = scripts::Store::load(paths)?;
        script_store.run(paths, &self.scripts, Order::Before)?;
        let mut directories = DirectoryModes::default();
        for change in &self.changes {
            if matches!(change.entry.content, Content::Directory)
                && change.entry.mode & 0o222 == 0
                && self.changes.iter().any(|child| {
                    child.action != Action::Unchanged
                        && child.entry.relative.starts_with(&change.entry.relative)
                })
            {
                let path = paths.destination.join(&change.entry.relative);
                if let Ok(meta) = fs::symlink_metadata(&path) {
                    if meta.is_dir() && !meta.file_type().is_symlink() {
                        filesystem::set_mode(&path, filesystem::path_mode(&path, &meta)? | 0o200)?;
                    }
                }
                directories.0.push((path, change.entry.mode));
            }
        }
        let mut backups = Backups::default();
        let mut applied = 0;
        let mut during = self
            .scripts
            .iter()
            .filter(|script| script.order == Order::During)
            .peekable();
        for original in &self.changes {
            while during
                .peek()
                .is_some_and(|script| script.sort_key <= original.entry.relative)
            {
                script_store.run(
                    paths,
                    std::slice::from_ref(during.next().unwrap()),
                    Order::During,
                )?;
            }
            let mut local = original.clone();
            let readonly_directory =
                matches!(local.entry.content, Content::Directory) && local.entry.mode & 0o222 == 0;
            if readonly_directory
                && directories
                    .0
                    .iter()
                    .any(|(path, _)| path == &paths.destination.join(&local.entry.relative))
            {
                if let Some(Fingerprint::Directory { mode }) = &mut local.actual.fingerprint {
                    *mode |= 0o200;
                }
            }
            let change = &local;
            let target = paths.destination.join(&change.entry.relative);
            let result = (|| -> Result<()> {
                check_parents(paths, &change.entry.relative, None)?;
                let now = snapshot(&target)?.fingerprint;
                if now != change.actual.fingerprint && now != change.entry.fingerprint() {
                    bail!("Destination changed since planning: {}", target.display());
                }
                if change.action != Action::Unchanged && now != change.entry.fingerprint() {
                    if let Some(actual) = &original.actual.fingerprint {
                        backups.save(paths, original, &target, actual)?;
                    }
                    write_entry(change, &target)?;
                    applied += 1;
                    println!("Applied {}", change.entry.relative.display());
                }
                if readonly_directory && directories.0.iter().any(|(path, _)| path == &target) {
                    return Ok(());
                }
                let key = change.entry.relative.to_str().unwrap().to_owned();
                let desired = change.entry.fingerprint();
                if state.entries.get(&key) != desired.as_ref() {
                    match desired {
                        Some(value) => {
                            state.entries.insert(key, value);
                        }
                        None => {
                            state.entries.remove(&key);
                        }
                    }
                    paths.save_state(state)?;
                }
                Ok(())
            })();
            result.with_context(|| format!("Deployment stopped after {applied} change(s); completed writes and backups were retained"))?;
        }
        for (path, mode) in &directories.0 {
            filesystem::set_mode(path, *mode)?;
            let relative = path
                .strip_prefix(&paths.destination)?
                .to_str()
                .unwrap()
                .to_owned();
            let fingerprint = Fingerprint::Directory { mode: *mode };
            if state.entries.get(&relative) != Some(&fingerprint) {
                state.entries.insert(relative, fingerprint);
                paths.save_state(state)?;
            }
        }
        directories.0.clear();
        for script in during {
            script_store.run(paths, std::slice::from_ref(script), Order::During)?;
        }
        script_store.run(paths, &self.scripts, Order::After)?;
        let managed: BTreeSet<_> = self
            .changes
            .iter()
            .map(|c| c.entry.relative.to_str().unwrap())
            .collect();
        let old_len = state.entries.len();
        state
            .entries
            .retain(|name, _| managed.contains(name.as_str()));
        if old_len != state.entries.len() {
            paths.save_state(state)?;
        }
        println!("Applied {applied} change(s)");
        Ok(())
    }
}

#[derive(Default)]
struct DirectoryModes(Vec<(PathBuf, u32)>);
impl Drop for DirectoryModes {
    fn drop(&mut self) {
        for (path, mode) in self.0.iter().rev() {
            let _ = filesystem::set_mode(path, *mode);
        }
    }
}

fn exact_extras(
    paths: &Paths,
    relative: &Path,
    entries: &BTreeMap<PathBuf, Entry>,
    removals: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let directory = paths.destination.join(relative);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(meta) => meta,
        Err(error) if absent(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(());
    }
    for child in fs::read_dir(directory)? {
        let child = child?;
        let path = relative.join(child.file_name());
        if entries.contains_key(&path) || entries.keys().any(|key| key.starts_with(&path)) {
            exact_extras(paths, &path, entries, removals)?;
        } else {
            paths.check_target(&paths.destination.join(&path))?;
            removals.insert(path);
        }
    }
    Ok(())
}

fn collect_removals(
    paths: &Paths,
    relative: &Path,
    matchers: &[globset::GlobMatcher],
    removals: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let directory = paths.destination.join(relative);
    if !directory.exists() {
        return Ok(());
    }
    for child in fs::read_dir(directory)? {
        let child = child?;
        let path = relative.join(child.file_name());
        let absolute = child.path();
        if matchers.iter().any(|matcher| matcher.is_match(&path)) {
            paths.check_target(&absolute)?;
            removals.insert(path);
            continue;
        }
        if absolute.starts_with(&paths.root) || absolute.starts_with(&paths.source) {
            continue;
        }
        let metadata = fs::symlink_metadata(&absolute)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            collect_removals(paths, &path, matchers, removals)?;
        }
    }
    Ok(())
}

fn describe(value: Option<&Fingerprint>) -> String {
    match value {
        None => "absent".into(),
        Some(Fingerprint::File { mode, .. }) => format!("file (mode {mode:04o})"),
        Some(Fingerprint::Directory { mode }) => format!("directory (mode {mode:04o})"),
        Some(Fingerprint::Symlink { target }) => format!("symlink -> {}", target.display()),
    }
}

#[cfg(windows)]
fn prepare_removal(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    filesystem::set_mode(path, if meta.is_dir() { 0o700 } else { 0o600 })?;
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            prepare_removal(&entry?.path())?;
        }
    }
    Ok(())
}
#[cfg(unix)]
fn prepare_removal(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        let mode = filesystem::mode(&meta);
        if mode & 0o700 != 0o700 {
            filesystem::set_mode(path, mode | 0o700)?;
        }
        for entry in fs::read_dir(path)? {
            prepare_removal(&entry?.path())?;
        }
    }
    Ok(())
}
#[cfg(not(any(unix, windows)))]
fn prepare_removal(_path: &Path) -> Result<()> {
    Ok(())
}

fn remove_leaf(path: &Path) -> Result<()> {
    prepare_removal(path)?;
    #[cfg(windows)]
    if fs::symlink_metadata(path)?.file_type().is_symlink() && path.is_dir() {
        fs::remove_dir(path)?;
        return Ok(());
    }
    fs::remove_file(path)?;
    Ok(())
}

fn write_entry(change: &Change, target: &Path) -> Result<()> {
    let actual_directory = matches!(
        change.actual.fingerprint,
        Some(Fingerprint::Directory { .. })
    );
    if actual_directory && !matches!(change.entry.content, Content::Directory) {
        prepare_removal(target)?;
        fs::remove_dir_all(target)?;
    }
    match &change.entry.content {
        Content::Directory => {
            if change.actual.fingerprint.is_some() && !actual_directory {
                remove_leaf(target)?;
            }
            fs::create_dir_all(target)?;
            filesystem::set_mode(
                target,
                if change.entry.mode & 0o222 == 0 {
                    change.entry.mode | 0o200
                } else {
                    change.entry.mode
                },
            )?;
        }
        Content::File(bytes) => filesystem::atomic_write(target, bytes, change.entry.mode)?,
        Content::Symlink(link) => {
            let temporary = tempfile::Builder::new()
                .prefix(".stool-link-")
                .tempdir_in(target.parent().unwrap())?;
            let staged = temporary.path().join("link");
            symlink(link, &staged, target.parent().unwrap())?;
            fs::rename(staged, target)?;
        }
        Content::Absent => {
            if change.actual.fingerprint.is_some() && !actual_directory {
                remove_leaf(target)?;
            }
        }
        Content::Create(_) | Content::Modify { .. } | Content::Remove { .. } => {
            unreachable!("entry was not planned")
        }
    }
    Ok(())
}

#[derive(Default)]
struct Backups {
    directory: Option<PathBuf>,
    records: BTreeMap<String, Fingerprint>,
}

impl Backups {
    fn save(
        &mut self,
        paths: &Paths,
        change: &Change,
        target: &Path,
        actual: &Fingerprint,
    ) -> Result<()> {
        if self.directory.is_none() {
            let root = paths.root.join("backups");
            filesystem::private_dir(&root)?;
            let directory = tempfile::Builder::new()
                .prefix("apply-")
                .tempdir_in(root)?
                .into_path();
            println!("Backups: {}", directory.display());
            self.directory = Some(directory);
        }
        let root = self.directory.as_ref().unwrap();
        if !(matches!(actual, Fingerprint::Directory { .. })
            && matches!(change.entry.content, Content::Directory))
        {
            let backup = root.join("files").join(&change.entry.relative);
            fs::create_dir_all(backup.parent().unwrap())?;
            copy_entry(target, &backup)?;
        }
        self.records.insert(
            change.entry.relative.to_str().unwrap().to_owned(),
            actual.clone(),
        );
        filesystem::atomic_write(
            &root.join("metadata.json"),
            &serde_json::to_vec_pretty(&self.records)?,
            0o600,
        )
    }
}

fn copy_entry(source: &Path, target: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        symlink(&fs::read_link(source)?, target, source.parent().unwrap())?;
    } else if metadata.is_dir() {
        fs::create_dir(target)?;
        for child in fs::read_dir(source)? {
            let child = child?;
            copy_entry(&child.path(), &target.join(child.file_name()))?;
        }
        filesystem::set_mode(target, filesystem::mode(&metadata))?;
    } else if metadata.is_file() {
        fs::copy(source, target)?;
    } else {
        bail!("Cannot back up special file: {}", source.display());
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, path: &Path, _base: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, path)?;
    Ok(())
}
#[cfg(windows)]
fn symlink(target: &Path, path: &Path, base: &Path) -> Result<()> {
    let resolved = if target.is_absolute() {
        target.to_owned()
    } else {
        base.join(target)
    };
    if resolved.is_dir() {
        std::os::windows::fs::symlink_dir(target, path)
    } else {
        std::os::windows::fs::symlink_file(target, path)
    }
    .context("Create symlink (enable Windows Developer Mode or grant symlink privileges)")?;
    Ok(())
}
#[cfg(not(any(unix, windows)))]
fn symlink(_target: &Path, _path: &Path, _base: &Path) -> Result<()> {
    bail!("Unsupported platform symlink")
}
