use super::source::{self, Entry, Kind};
use super::storage::{self, Paths};
use super::template::Engine;
use super::{crypto, external};
use crate::cli::dotfile::{AddArgs, EditArgs};
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn target_relative(paths: &Paths, argument: &Path) -> Result<PathBuf> {
    let expanded = filesystem::expand_home(argument, &paths.home);
    let relative = if expanded.is_absolute() {
        expanded
            .strip_prefix(&paths.destination)
            .context("Path is outside the selected destination")?
            .to_owned()
    } else {
        expanded
    };
    let relative = storage::relative_path(&relative)?;
    paths.check_target(&paths.destination.join(&relative))?;
    let mut parent = relative.parent();
    while let Some(path) = parent {
        let absolute = paths.destination.join(path);
        if let Ok(metadata) = fs::symlink_metadata(&absolute) {
            if metadata.file_type().is_symlink() {
                bail!("Destination parent is a symlink: {}", absolute.display());
            }
        }
        parent = path.parent();
    }
    Ok(relative)
}

fn encode(name: &str, directory: bool) -> String {
    let mut result = if let Some(rest) = name.strip_prefix('.') {
        format!("dot_{rest}")
    } else {
        name.to_owned()
    };
    let reserved = [
        "encrypted_",
        "private_",
        "readonly_",
        "empty_",
        "executable_",
        "create_",
        "modify_",
        "remove_",
        "run_",
        "exact_",
        "external_",
        "literal_",
        "symlink_",
        "once_",
        "onchange_",
        "before_",
        "after_",
        "dot_",
    ];
    if let Some(rest) = name.strip_prefix('.') {
        if reserved.iter().any(|prefix| rest.starts_with(prefix)) {
            result = format!("dot_literal_{rest}");
        }
    } else if reserved.iter().any(|prefix| name.starts_with(prefix)) {
        result = format!("literal_{name}");
    }
    if !directory && (name.ends_with(".tmpl") || name.ends_with(".literal")) {
        result.push_str(".literal");
    }
    result
}

fn literal_directory(root: &Path, directory: &Path) -> Result<bool> {
    let relative = directory.strip_prefix(root)?;
    for part in relative.components() {
        if source::attributes(
            part.as_os_str().to_str().context("Non-UTF-8 source name")?,
            true,
        )?
        .external
        {
            return Ok(true);
        }
    }
    Ok(false)
}

struct Import {
    target: PathBuf,
    source: PathBuf,
    bytes: Option<Vec<u8>>,
    old: Option<PathBuf>,
}

pub fn add(paths: &Paths, engine: &Engine, args: &AddArgs) -> Result<()> {
    let mut tree = source::scan(paths, engine, false)?;
    external::load(paths, engine, &mut tree, false)?;
    let root = tree.root;
    let known = tree.entries;
    let mut imports = BTreeMap::new();
    for path in &args.paths {
        let relative = target_relative(paths, path)?;
        import(paths, &root, &known, &relative, args, &mut imports)?;
    }
    let mut state = paths.state()?;
    for (relative, operation) in &imports {
        if let Some(bytes) = &operation.bytes {
            fs::create_dir_all(operation.source.parent().unwrap())?;
            filesystem::atomic_write(&operation.source, bytes, 0o600)?;
            if let Some(old) = &operation.old {
                if old != &operation.source {
                    fs::remove_file(old)?;
                }
            }
        } else {
            fs::create_dir_all(&operation.source)?;
        }
        let fingerprint = super::plan::actual_fingerprint(&operation.target)?;
        if let Some(fingerprint) = fingerprint {
            state
                .entries
                .insert(relative.to_str().unwrap().to_owned(), fingerprint);
        }
        paths.save_state(&state)?;
        println!(
            "Added {} -> {}",
            operation.target.display(),
            operation.source.display()
        );
    }
    Ok(())
}

fn import(
    paths: &Paths,
    root: &Path,
    known: &BTreeMap<PathBuf, Entry>,
    relative: &Path,
    args: &AddArgs,
    output: &mut BTreeMap<PathBuf, Import>,
) -> Result<()> {
    let target = paths.destination.join(relative);
    paths.check_target(&target)?;
    let metadata =
        fs::symlink_metadata(&target).with_context(|| format!("Read {}", target.display()))?;
    let mut source_parent = root.to_owned();
    let mut target_parent = PathBuf::new();
    for part in relative.parent().unwrap_or(Path::new("")).components() {
        target_parent.push(part.as_os_str());
        if let Some(operation) = output.get(&target_parent) {
            source_parent = operation.source.clone();
        } else if let Some(origin) = known
            .get(&target_parent)
            .and_then(|entry| entry.origin.as_ref())
        {
            source_parent = origin.clone();
        } else {
            let name = part
                .as_os_str()
                .to_str()
                .context("Source names must be UTF-8")?;
            let mode = fs::metadata(paths.destination.join(&target_parent))
                .map(|meta| filesystem::mode(&meta))
                .unwrap_or(0o755);
            source_parent.push(format!(
                "{}{}",
                if mode & 0o077 == 0 { "private_" } else { "" },
                encode(name, true)
            ));
        }
    }
    if literal_directory(root, &source_parent)? {
        bail!(
            "Add inside external_ directories is ambiguous; edit the literal source files directly"
        );
    }
    let filename = relative
        .file_name()
        .unwrap()
        .to_str()
        .context("Source names must be UTF-8")?;
    if known
        .get(relative)
        .is_some_and(|entry| entry.origin.is_none())
    {
        bail!("External entries cannot be imported; change the external resource definition");
    }
    let existing = known.get(relative).and_then(|entry| entry.origin.as_ref());
    let previous = existing
        .map(|path| {
            source::attributes(
                path.file_name().unwrap().to_str().unwrap(),
                metadata.is_dir(),
            )
        })
        .transpose()?;
    if previous
        .as_ref()
        .is_some_and(|attr| matches!(attr.kind, Kind::Modify | Kind::Remove | Kind::Script(..)))
    {
        bail!(
            "Use edit to change scripted/removal source entries: {}",
            relative.display()
        );
    }
    let encrypted = args.encrypt || previous.as_ref().is_some_and(|attr| attr.encrypted);
    let templated = args.template || previous.as_ref().is_some_and(|attr| attr.template);
    let mode = filesystem::path_mode(&target, &metadata)?;
    let mut name = encode(filename, metadata.is_dir());
    let bytes = if metadata.file_type().is_symlink() {
        if encrypted || templated {
            bail!("add --encrypt/--template do not apply to native symlinks");
        }
        name = format!("symlink_{name}");
        Some(
            fs::read_link(&target)?
                .to_str()
                .context("Symlink target must be UTF-8")?
                .as_bytes()
                .to_vec(),
        )
    } else if metadata.is_dir() {
        if mode & 0o077 == 0 {
            name = format!("private_{name}");
        }
        if mode & 0o222 == 0 {
            name = name.replacen("private_", "private_readonly_", 1);
            if mode & 0o077 != 0 {
                name = format!("readonly_{name}");
            }
        }
        None
    } else if metadata.is_file() {
        let mut bytes = fs::read(&target)?;
        if templated {
            std::str::from_utf8(&bytes).context("A template source must be UTF-8")?;
            name.push_str(".tmpl");
        }
        let mut prefixes = String::new();
        if encrypted {
            prefixes.push_str("encrypted_");
        }
        if mode & 0o077 == 0 {
            prefixes.push_str("private_");
        }
        if mode & 0o222 == 0 {
            prefixes.push_str("readonly_");
        }
        if bytes.is_empty() {
            prefixes.push_str("empty_");
        }
        if cfg!(unix) && mode & 0o111 != 0 {
            prefixes.push_str("executable_");
        }
        name = format!("{prefixes}{name}");
        if encrypted {
            let mut settings = paths.encryption();
            if let Some(previous) = &previous {
                if previous.encrypted {
                    settings.backend = if previous.encryption_extension == "asc" {
                        "gpg"
                    } else {
                        "age"
                    }
                    .into();
                }
            }
            bytes = crypto::encrypt(&settings, &bytes)?;
            name.push('.');
            name.push_str(crypto::suffix(&settings)?);
        }
        Some(bytes)
    } else {
        bail!("Cannot add special file: {}", target.display());
    };
    let source = if let Some(existing) = existing {
        if previous.as_ref().is_some_and(|attr| {
            attr.encrypted == encrypted
                && attr.template == templated
                && !matches!(attr.kind, Kind::Symlink)
        }) {
            existing.clone()
        } else {
            source_parent.join(name)
        }
    } else {
        source_parent.join(name)
    };
    let source = if metadata.is_dir() {
        existing.cloned().unwrap_or(source)
    } else {
        source
    };
    if !source.starts_with(root) {
        bail!("Source path escapes repository");
    }
    output.insert(
        relative.to_owned(),
        Import {
            target,
            source,
            bytes,
            old: existing.cloned(),
        },
    );
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        for child in fs::read_dir(paths.destination.join(relative))? {
            let child = child?;
            import(
                paths,
                root,
                known,
                &relative.join(child.file_name()),
                args,
                output,
            )?;
        }
    }
    Ok(())
}

pub fn edit(paths: &Paths, engine: &Engine, args: &EditArgs) -> Result<()> {
    let entries = source::scan(paths, engine, false)?.entries;
    let editor = paths
        .config
        .as_ref()
        .and_then(|cfg| cfg.editor.clone())
        .or_else(|| std::env::var("VISUAL").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad.exe".into()
            } else {
                "vi".into()
            }
        });
    let argv = shell_words::split(&editor)?;
    let executable = argv.first().context("Editor command is empty")?;
    for argument in &args.paths {
        let relative = target_relative(paths, argument)?;
        let entry = entries
            .get(&relative)
            .context("Path is not managed; add it first")?;
        let origin = entry
            .origin
            .as_ref()
            .context("External entries cannot be edited as source files")?;
        if literal_directory(
            &super::template::source_root(paths)?,
            origin.parent().unwrap(),
        )? {
            let status = Command::new(executable)
                .args(&argv[1..])
                .arg(origin)
                .status()
                .context("Start editor")?;
            if !status.success() {
                bail!("Editor failed");
            }
            continue;
        }
        let attr = source::attributes(
            origin.file_name().unwrap().to_str().unwrap(),
            origin.is_dir(),
        )?;
        if attr.encrypted {
            let directory = tempfile::Builder::new().prefix("stool-edit-").tempdir()?;
            let plaintext = directory.path().join(attr.name);
            let bytes = crypto::decrypt(
                &paths.encryption(),
                &paths.home,
                &fs::read(origin)?,
                &attr.encryption_extension,
            )?;
            filesystem::atomic_write(&plaintext, &bytes, 0o600)?;
            let status = Command::new(executable)
                .args(&argv[1..])
                .arg(&plaintext)
                .status()
                .context("Start editor")?;
            if !status.success() {
                bail!("Editor failed; encrypted source was preserved");
            }
            let mut settings = paths.encryption();
            settings.backend = if attr.encryption_extension == "asc" {
                "gpg"
            } else {
                "age"
            }
            .into();
            let encrypted = crypto::encrypt(&settings, &fs::read(plaintext)?)?;
            filesystem::atomic_write(origin, &encrypted, 0o600)?;
        } else {
            let status = Command::new(executable)
                .args(&argv[1..])
                .arg(origin)
                .status()
                .context("Start editor")?;
            if !status.success() {
                bail!("Editor failed");
            }
        }
        println!("Edited {}", origin.display());
    }
    Ok(())
}

pub fn unmanaged(paths: &Paths, engine: &Engine) -> Result<()> {
    let mut tree = source::scan(paths, engine, false)?;
    external::load(paths, engine, &mut tree, false)?;
    let entries = tree.entries;
    walk_unmanaged(paths, Path::new(""), &entries)
}
fn walk_unmanaged(
    paths: &Paths,
    relative: &Path,
    entries: &BTreeMap<PathBuf, Entry>,
) -> Result<()> {
    let directory = paths.destination.join(relative);
    if !directory.exists() {
        return Ok(());
    }
    let mut children = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = relative.join(child.file_name());
        let absolute = child.path();
        if absolute.starts_with(&paths.source) || absolute.starts_with(&paths.root) {
            continue;
        }
        if !paths.root.starts_with(&absolute)
            && !paths.source.starts_with(&absolute)
            && !entries.contains_key(&path)
            && !entries.keys().any(|name| name.starts_with(&path))
        {
            println!("{}", absolute.display());
        }
        let metadata = fs::symlink_metadata(&absolute)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            walk_unmanaged(paths, &path, entries)?;
        }
    }
    Ok(())
}
