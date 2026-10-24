use super::plan::Plan;
use super::source::{Content, Entry};
use super::storage;
use crate::cli::dotfile::ArchiveFormat;
use crate::common::fs as filesystem;
use anyhow::{bail, Context, Result};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

pub fn export(plan: &Plan, format: ArchiveFormat, output: &Path) -> Result<()> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    match format {
        ArchiveFormat::Tar => tar(plan, &mut temporary)?,
        ArchiveFormat::Tgz => {
            let mut encoder =
                flate2::write::GzEncoder::new(&mut temporary, flate2::Compression::default());
            tar(plan, &mut encoder)?;
            encoder.finish()?;
        }
        ArchiveFormat::Zip => {
            let mut writer = zip::ZipWriter::new(&mut temporary);
            for change in &plan.changes {
                let entry = &change.entry;
                let name = entry
                    .relative
                    .to_str()
                    .context("Archive paths must be UTF-8")?
                    .replace('\\', "/");
                let options = zip::write::FileOptions::default()
                    .unix_permissions(entry.mode)
                    .compression_method(zip::CompressionMethod::Deflated);
                match &entry.content {
                    Content::Directory => writer.add_directory(format!("{name}/"), options)?,
                    Content::File(bytes) => {
                        writer.start_file(name, options)?;
                        writer.write_all(bytes)?;
                    }
                    Content::Symlink(target) => writer.add_symlink(
                        name,
                        target.to_str().context("Symlink target must be UTF-8")?,
                        options,
                    )?,
                    Content::Absent => {}
                    _ => unreachable!(),
                }
            }
            writer.finish()?;
        }
    }
    temporary.as_file().sync_all()?;
    filesystem::set_mode(temporary.path(), 0o600)?;
    temporary.persist(output).map_err(|error| error.error)?;
    println!("Archive: {}", output.display());
    Ok(())
}

fn tar(plan: &Plan, writer: impl Write) -> Result<()> {
    let mut archive = tar::Builder::new(writer);
    for change in &plan.changes {
        let entry = &change.entry;
        let mut header = tar::Header::new_gnu();
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_mode(entry.mode);
        match &entry.content {
            Content::Directory => {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                archive.append_data(&mut header, &entry.relative, std::io::empty())?;
            }
            Content::File(bytes) => {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(bytes.len() as u64);
                header.set_cksum();
                archive.append_data(&mut header, &entry.relative, bytes.as_slice())?;
            }
            Content::Symlink(target) => {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_link_name(target)?;
                header.set_cksum();
                archive.append_data(&mut header, &entry.relative, std::io::empty())?;
            }
            Content::Absent => {}
            _ => unreachable!(),
        }
    }
    archive.finish()?;
    Ok(())
}

pub fn read(bytes: &[u8], strip: usize) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            let path = file
                .enclosed_name()
                .context("Archive path escapes its root")?
                .to_owned();
            let Some(relative) = stripped(&path, strip)? else {
                continue;
            };
            let mode = file
                .unix_mode()
                .unwrap_or(if file.is_dir() { 0o755 } else { 0o644 });
            let content = if file.is_dir() {
                Content::Directory
            } else {
                let mut data = Vec::new();
                file.read_to_end(&mut data)?;
                if mode & 0o170000 == 0o120000 {
                    Content::Symlink(PathBuf::from(String::from_utf8(data)?))
                } else {
                    Content::File(data)
                }
            };
            entries.push(Entry::new(relative, content, mode & 0o777));
        }
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        read_tar(flate2::read::GzDecoder::new(bytes), strip, &mut entries)?;
    } else {
        read_tar(bytes, strip, &mut entries)?;
    }
    let mut kinds = std::collections::BTreeMap::new();
    for entry in &entries {
        if kinds
            .insert(
                entry.relative.clone(),
                matches!(entry.content, Content::Directory),
            )
            .is_some()
        {
            bail!(
                "Archive contains duplicate path {}",
                entry.relative.display()
            );
        }
    }
    for entry in &entries {
        let mut parent = entry.relative.parent();
        while let Some(path) = parent {
            if kinds.get(path) == Some(&false) {
                bail!(
                    "Archive path traverses a file or symlink: {}",
                    entry.relative.display()
                );
            }
            parent = path.parent();
        }
    }
    Ok(entries)
}
fn stripped(path: &Path, strip: usize) -> Result<Option<PathBuf>> {
    let path = storage::relative_path(path)?;
    let parts: Vec<_> = path.components().collect();
    if parts.len() <= strip {
        return Ok(None);
    }
    Ok(Some(parts[strip..].iter().collect()))
}
fn read_tar(reader: impl Read, strip: usize, output: &mut Vec<Entry>) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    for member in archive.entries()? {
        let mut member = member?;
        let path = member.path()?.into_owned();
        if path == Path::new(".") {
            continue;
        }
        let Some(relative) = stripped(&path, strip)? else {
            continue;
        };
        let mode = member.header().mode()? & 0o777;
        let kind = member.header().entry_type();
        let content = if kind.is_dir() {
            Content::Directory
        } else if kind.is_file() {
            let mut bytes = Vec::new();
            member.read_to_end(&mut bytes)?;
            Content::File(bytes)
        } else if kind.is_symlink() {
            Content::Symlink(
                member
                    .link_name()?
                    .context("Symlink has no target")?
                    .into_owned(),
            )
        } else if kind.is_hard_link() {
            let link = member
                .link_name()?
                .context("Hard link has no target")?
                .into_owned();
            let link =
                stripped(&link, strip)?.context("Hard link points outside stripped archive")?;
            match output
                .iter()
                .find(|entry| entry.relative == link)
                .map(|entry| &entry.content)
            {
                Some(Content::File(bytes)) => Content::File(bytes.clone()),
                _ => bail!("Archive hard link must reference an earlier regular file"),
            }
        } else {
            bail!(
                "Archive contains unsupported special file: {}",
                relative.display()
            );
        };
        output.push(Entry::new(relative, content, mode));
    }
    Ok(())
}
